//! The row vocabulary every OS table is written in.
use crate::guard::Kind;

/// Everything one OS refuses or allows by policy.
pub(super) struct Table {
    /// Guard layer 2 (`safety-invariants.md` section 2.1, and analyze's structural list, 2.7).
    pub critical: &'static [Row],
    /// Guard layer 4 (sections 2.4, 2.5, 2.7).
    pub protection: &'static [Row],
    /// Allow rows that join both layer 2 and layer 4: Mole's early carve-outs (section 2.2). A
    /// deeper refuse row still wins, and live-state refusals still apply under them (SAFE-06).
    /// Allow rows match by spelling only.
    pub carve_outs: &'static [Row],
    pub names: Names,
    pub live: Live,
    /// The mount point of the data volume, and the paths below it that the root also reaches
    /// (`/usr/share/firmlinks`). Each is mapped only after the guard verifies that the root
    /// entry and the data-volume entry are one directory.
    pub data_volume: Option<(&'static str, &'static [&'static str])>,
    /// Compare names without ASCII case when matching by spelling. What folding misses (case
    /// folds outside ASCII, Unicode normalization, 8.3 names) is caught by the anchor's file
    /// identity and by the stored spelling of the path.
    pub fold_case: bool,
}

#[derive(Clone, Copy)]
pub(super) struct Row {
    pub at: Anchor,
    pub scope: Scope,
    pub effect: Effect,
}

/// Where a row sits. A component that contains `*` is a glob over one name.
#[derive(Clone, Copy)]
pub(super) enum Anchor {
    /// An absolute spelling.
    Abs(&'static str),
    /// Relative to the home folder; `""` is the home folder itself.
    Home(&'static str),
    /// `%VAR%` followed by a suffix. Skipped when the variable is unset. These rows sit beside
    /// literal rows and only add refusals, so the environment cannot remove protection.
    Env(&'static str, &'static str),
    /// A run of whole components anywhere in a path, such as `Microsoft\Credentials`.
    Segments(&'static [&'static str]),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Scope {
    /// The anchor itself.
    Exact,
    /// The anchor and everything below it.
    Subtree,
    /// Everything below the anchor, not the anchor.
    Below,
    /// Exactly one level below the anchor.
    Children,
}

#[derive(Clone, Copy)]
pub(super) enum Effect {
    Refuse(Kind),
    /// Refuse, and refuse every ancestor that contains the anchor while the anchor exists
    /// (SAFE-27: Docker and OrbStack state, Mole's analyze rule).
    RefuseWithAncestors(Kind),
    Allow,
    /// Allow, and switch bundle-id patterns off below the anchor. Mole lets a container's cache
    /// and temp folders through although the container's own id is protected
    /// (lib/core/app_protection.sh:495-511, 657-660). Keywords and deeper rows still apply.
    AllowBundles,
}

/// Rules that match a name, not a place (layer 4).
pub(super) struct Names {
    /// Bundle-id globs, folded like every other name when `fold_case` is set. A match on the
    /// last component refuses that path. A match
    /// on a folder below one of `bundle_parents` refuses the whole folder. A `*x*` glob refuses
    /// every path with a component that contains `x`.
    pub bundle_globs: &'static [&'static [&'static str]],
    /// Folders whose children are named by bundle id, as runs of components.
    pub bundle_parents: &'static [&'static [&'static str]],
    /// Substrings that refuse any path with a component that contains one.
    pub keywords: &'static [(&'static str, Kind)],
}

/// Live-state rules (layer 3). Pure shape tests decide which paths need a probe at all.
pub(super) struct Live {
    /// Home-relative scopes whose `*` component is an owner bundle id
    /// (`_mole_user_cache_scope`, lib/core/file_ops.sh:206-248). Reverse-DNS names only.
    pub user_cache_scopes: &'static [&'static str],
    /// Home-relative scopes whose `*` component is an app group's folder, which several apps
    /// can share.
    pub group_cache_scopes: &'static [&'static str],
    /// A `Current*` file one level below a store here, with `-wal` and `-shm`, is always live.
    pub powerlog_stores: Option<&'static str>,
    /// Endpoint-security caches: the root, and vendor prefixes matched case-insensitively.
    pub edr: Option<(&'static str, &'static [&'static str])>,
    /// SQLite main-file suffixes. Companions add `-wal`, `-shm` and `-journal`.
    pub sqlite_suffixes: &'static [&'static str],
    /// Home-relative folder, and the suffixes of partial downloads one level inside it.
    pub partial_downloads: &'static [(&'static str, &'static [&'static str])],
}

pub(super) const fn row(at: Anchor, scope: Scope, effect: Effect) -> Row {
    Row { at, scope, effect }
}
