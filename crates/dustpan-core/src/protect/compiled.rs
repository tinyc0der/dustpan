//! A table compiled for one OS and one user: two tries walked one component at a time, name
//! patterns, and the file identity of every protected anchor. Built once by `Guard::new`;
//! read-only afterwards.
//!
//! One contest decides layers 2 and 4. Every rule that matches a path enters with the depth of
//! the component it matched. The deepest wins. At equal depth a refuse row beats an allow row,
//! and an allow row beats a name pattern.
use super::live::{LiveChecks, LiveRules};
use super::names::{BundleMatch, Comp, Glob, NamePatterns, fold};
use super::table::{Anchor, Effect, Row, Scope, Table};
use super::whitelisted::Whitelisted;
use crate::guard::{GuardInputs, Hit, How, Kind, PolicyError};
use crate::identity::FileId;
use crate::path::AbsPath;
use crate::probe::{self, Probe, Seen};
use std::borrow::Cow;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{MAIN_SEPARATOR, PathBuf};

const SEP: u8 = MAIN_SEPARATOR as u8;

/// The file-system top: `/` on Unix, the parent of every drive on Windows.
const ORIGIN: u32 = 0;
/// Where rows that match a run of components anywhere in a path start.
const FLOATING: u32 = 1;

const RANK_NAME: u8 = 1;
const RANK_ALLOW: u8 = 2;
const RANK_REFUSE: u8 = 3;

pub(crate) struct Policy {
    fold_case: bool,
    critical: Trie,
    protection: Trie,
    names: NamePatterns,
    /// Every anchored refuse row whose anchor exists on this host, keyed by the anchor's identity.
    /// Identity only adds refusals. Allow rows match by spelling alone, so an alias can refuse
    /// a path but never grant one (SAFE-04, SAFE-05).
    ///
    /// Taken once per process. It holds no per-candidate fact, only where the protected anchors
    /// are, and an entry can only refuse: a replaced anchor leaves an entry no path matches, or
    /// one a reused inode matches, which over-refuses. The anchor's own spelling stays covered
    /// by the trie either way.
    identity: HashMap<FileId, Vec<u32>>,
    id_rows: Vec<IdRow>,
    data_volume: Option<DataVolume>,
    live: LiveRules,
    pub(crate) whitelist: Whitelisted,
}

struct IdRow {
    which: Which,
    scope: Scope,
    kind: Kind,
    anchor: AbsPath,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Which {
    Critical,
    Protection,
}

#[derive(Default)]
struct Trie {
    nodes: Vec<TrieNode>,
}

#[derive(Default)]
struct TrieNode {
    exact: HashMap<Box<[u8]>, u32>,
    globs: Vec<(Glob, u32)>,
    rules: Vec<(Scope, Outcome)>,
    /// Protected state below this node that exists only while its anchor does (SAFE-27).
    inside: Vec<Inside>,
    /// The children of this folder are named by bundle id.
    bundle_parent: bool,
}

pub(crate) struct Inside {
    pub inner: AbsPath,
    pub kind: Kind,
    /// The anchor is a glob, so it cannot be probed and its ancestors refuse outright.
    pub certain: bool,
}

#[derive(Clone, Copy)]
enum Outcome {
    Refuse(Kind),
    Allow { bundles_off: bool },
}

impl Outcome {
    const fn rank(self) -> u8 {
        match self {
            Self::Refuse(_) => RANK_REFUSE,
            Self::Allow { .. } => RANK_ALLOW,
        }
    }
}

/// One entry in the contest: what matched, and how deep.
#[derive(Clone, Copy)]
struct Won {
    depth: u16,
    rank: u8,
    outcome: Outcome,
    via: Via,
}

#[derive(Clone, Copy)]
enum Via {
    Row,
    ChildRow,
    SameFile(u32),
    Bundle,
    Keyword,
}

fn enter(best: &mut Option<Won>, new: Won) {
    if best.is_none_or(|b| (new.depth, new.rank) > (b.depth, b.rank)) {
        *best = Some(new);
    }
}

fn bundles_off(won: Option<Won>) -> bool {
    matches!(
        won,
        Some(Won {
            outcome: Outcome::Allow { bundles_off: true },
            ..
        })
    )
}

/// Where one directory sits in each trie, and the nearest rule it passes down. Lives in a `Pass`
/// frame, so every path below the directory reuses it.
pub(crate) struct Step {
    depth: u16,
    critical: Position,
    protection: Position,
    /// Identity rows that refuse this directory's direct children.
    child_rows: Vec<u32>,
    bundles: Bundles,
}

/// Whether the bundle-id name patterns take part in the contest. A bundle id names an app's own
/// folder. Inside a candidate the guard approved, a folder named for an app is part of the
/// candidate, so the survey of a candidate's contents passes `Ignored`. Every row anchored on a
/// path still applies there.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Bundles {
    Named,
    Ignored,
}

#[derive(Default)]
struct Position {
    nodes: Vec<u32>,
    inherit: Option<Won>,
}

/// Layers 2 and 4 for one leaf.
pub(crate) struct Ruling<'p> {
    pub critical: Option<Hit>,
    pub protection: Option<Hit>,
    /// Anchors under the leaf whose rows refuse their ancestors while they exist (SAFE-27).
    pub inside: Vec<&'p Inside>,
}

struct DataVolume {
    root: AbsPath,
    /// Folded, longest first, each verified by identity.
    firmlinks: Vec<Vec<u8>>,
}

impl Trie {
    fn new() -> Self {
        Self {
            nodes: vec![TrieNode::default(), TrieNode::default()],
        }
    }

    /// The node `comps` names below `start`, created as needed, and the nodes passed on the way
    /// to it, `start` first.
    fn insert(&mut self, start: u32, comps: Vec<Comp>) -> (u32, Vec<u32>) {
        let mut above = Vec::with_capacity(comps.len());
        let mut at = start;
        for comp in comps {
            let node = &self.nodes[at as usize];
            let found = match &comp {
                Comp::Lit(name) => node.exact.get(name).copied(),
                Comp::Glob(glob) => node.globs.iter().find(|(g, _)| g.0 == glob.0).map(|e| e.1),
            };
            let next = found.unwrap_or_else(|| {
                self.nodes.push(TrieNode::default());
                let new = (self.nodes.len() - 1) as u32;
                match comp {
                    Comp::Lit(name) => drop(self.nodes[at as usize].exact.insert(name, new)),
                    Comp::Glob(glob) => self.nodes[at as usize].globs.push((glob, new)),
                }
                new
            });
            above.push(at);
            at = next;
        }
        (at, above)
    }

    fn each_child<'t>(&'t self, at: u32, name: &[u8], mut visit: impl FnMut(&'t TrieNode, u32)) {
        let node = &self.nodes[at as usize];
        if let Some(&child) = node.exact.get(name) {
            visit(&self.nodes[child as usize], child);
        }
        for (glob, child) in &node.globs {
            if glob.matches(name) {
                visit(&self.nodes[*child as usize], *child);
            }
        }
    }
}

fn has_glob(path: &AbsPath) -> bool {
    path.bytes().contains(&b'*')
}

impl Policy {
    pub(crate) fn compile(
        inputs: &GuardInputs<'_>,
        probe: &dyn Probe,
    ) -> Result<Self, PolicyError> {
        Self::from_table(super::table(inputs.os), inputs, probe)
    }

    pub(super) fn from_table(
        table: &Table,
        inputs: &GuardInputs<'_>,
        probe: &dyn Probe,
    ) -> Result<Self, PolicyError> {
        let mut policy = Self {
            fold_case: table.fold_case,
            critical: Trie::new(),
            protection: Trie::new(),
            names: NamePatterns::new(table.fold_case),
            identity: HashMap::new(),
            id_rows: Vec::new(),
            data_volume: None,
            live: LiveRules::compile(table, inputs)?,
            whitelist: Whitelisted::empty(table.fold_case),
        };
        let is_home = |row: &&Row| matches!(row.at, Anchor::Home(_));
        let rooted = table.critical.iter().filter(|r| !is_home(r));
        for row in rooted.chain(table.carve_outs) {
            policy.add_row(Which::Critical, row, inputs)?;
        }
        // `HOME=/` or `HOME=/Users` would anchor every `~` row on a system tree.
        for home in inputs.homes.iter() {
            if let Some(kind) = policy.critical_by_spelling(home)
                && kind != Kind::HomeRoot
            {
                return Err(PolicyError::HomeIsCritical(home.clone()));
            }
        }
        for row in table.critical.iter().filter(is_home) {
            policy.add_row(Which::Critical, row, inputs)?;
        }
        for row in table.protection.iter().chain(table.carve_outs) {
            policy.add_row(Which::Protection, row, inputs)?;
        }
        for segments in table.names.bundle_parents {
            let comps = segments
                .iter()
                .map(|s| Comp::new(s.as_bytes(), table.fold_case))
                .collect();
            let (node, _) = policy.protection.insert(FLOATING, comps);
            policy.protection.nodes[node as usize].bundle_parent = true;
        }
        for pattern in table.names.bundle_globs.iter().flat_map(|list| list.iter()) {
            policy.names.add_bundle(pattern)?;
        }
        for &(keyword, kind) in table.names.keywords {
            let folded = fold(keyword.as_bytes(), table.fold_case);
            policy.names.add_keyword(folded, kind);
        }

        let refusing = [
            (Which::Critical, table.critical),
            (Which::Protection, table.protection),
        ];
        for (which, rows) in refusing {
            for row in rows {
                policy.bind_identity(which, row, inputs, probe)?;
            }
        }
        policy.data_volume = Self::verified_firmlinks(table, probe)?;
        policy.whitelist = Whitelisted::compile(
            &inputs.whitelist,
            table.fold_case,
            |path| policy.canonical(path).into_owned(),
            probe,
        )?;
        Ok(policy)
    }

    fn fold_into(&self, bytes: &[u8], out: &mut Vec<u8>) {
        out.clear();
        out.extend_from_slice(bytes);
        if self.fold_case {
            out.make_ascii_lowercase();
        }
    }

    fn comps(&self, path: &AbsPath) -> Vec<Comp> {
        path.names()
            .map(|n| Comp::new(n.as_encoded_bytes(), self.fold_case))
            .collect()
    }

    /// The paths a row is anchored at: one per home for a `~` row, none for a floating row or
    /// for an env row whose variable is unset or not an absolute path.
    fn anchors(row: &Row, inputs: &GuardInputs<'_>) -> Result<Vec<AbsPath>, PolicyError> {
        match row.at {
            Anchor::Abs(text) => AbsPath::parse(text.as_ref())
                .map(|path| vec![path])
                .map_err(|_| PolicyError::BadAnchor(text)),
            Anchor::Home(rel) => inputs
                .homes
                .iter()
                .map(|home| {
                    let mut path = home.as_path().to_path_buf();
                    path.extend(rel.split(['/', '\\']).filter(|part| !part.is_empty()));
                    AbsPath::parse(path.as_os_str()).map_err(|_| PolicyError::BadAnchor(rel))
                })
                .collect(),
            Anchor::Env(var, suffix) => Ok((inputs.env)(var)
                .filter(|value| !value.is_empty())
                .and_then(|value| AbsPath::parse(format!("{value}{suffix}").as_ref()).ok())
                .into_iter()
                .collect()),
            Anchor::Segments(_) => Ok(Vec::new()),
        }
    }

    fn add_row(
        &mut self,
        which: Which,
        row: &Row,
        inputs: &GuardInputs<'_>,
    ) -> Result<(), PolicyError> {
        if let Anchor::Segments(segments) = row.at {
            let comps = segments
                .iter()
                .map(|s| Comp::new(s.as_bytes(), self.fold_case))
                .collect();
            return self.insert_row(which, row, FLOATING, comps, None);
        }
        for anchor in Self::anchors(row, inputs)? {
            let comps = self.comps(&anchor);
            self.insert_row(which, row, ORIGIN, comps, Some(anchor))?;
        }
        Ok(())
    }

    fn insert_row(
        &mut self,
        which: Which,
        row: &Row,
        start: u32,
        comps: Vec<Comp>,
        anchor: Option<AbsPath>,
    ) -> Result<(), PolicyError> {
        let (outcome, guards_ancestors) = match row.effect {
            Effect::Refuse(kind) => (Outcome::Refuse(kind), None),
            Effect::RefuseWithAncestors(kind) => (Outcome::Refuse(kind), Some(kind)),
            Effect::Allow => (Outcome::Allow { bundles_off: false }, None),
            Effect::AllowBundles => (Outcome::Allow { bundles_off: true }, None),
        };
        let trie = match which {
            Which::Critical => &mut self.critical,
            Which::Protection => &mut self.protection,
        };
        let (at, ancestors) = trie.insert(start, comps);
        trie.nodes[at as usize].rules.push((row.scope, outcome));
        if let Some(kind) = guards_ancestors {
            let Some(anchor) = anchor else {
                return Err(PolicyError::BadAnchor(
                    "a floating row cannot guard its ancestors",
                ));
            };
            for node in ancestors {
                trie.nodes[node as usize].inside.push(Inside {
                    inner: anchor.clone(),
                    kind,
                    certain: has_glob(&anchor),
                });
            }
        }
        Ok(())
    }

    fn bind_identity(
        &mut self,
        which: Which,
        row: &Row,
        inputs: &GuardInputs<'_>,
        probe: &dyn Probe,
    ) -> Result<(), PolicyError> {
        let (Effect::Refuse(kind) | Effect::RefuseWithAncestors(kind)) = row.effect else {
            return Ok(());
        };
        for anchor in Self::anchors(row, inputs)? {
            if has_glob(&anchor) {
                continue;
            }
            // The folders above the anchor are resolved, the anchor itself is not. An anchor that
            // is a link (`C:\Documents and Settings` to `C:\Users`, `/etc` to `/private/etc`)
            // binds the link, so its row never spreads to the folder behind it. That folder has
            // its own rows.
            let node = match (anchor.parent(), anchor.name()) {
                (Some(parent), Some(name)) => match probe::physical(probe, &parent) {
                    Seen::Found(real) => probe.node(&AbsPath::trusted(real.as_path().join(name))),
                    Seen::Missing => Seen::Missing,
                    Seen::Unknown(why) => Seen::Unknown(why),
                },
                _ => probe.node(&anchor),
            };
            match node {
                Seen::Found(node) => {
                    self.id_rows.push(IdRow {
                        which,
                        scope: row.scope,
                        kind,
                        anchor,
                    });
                    let index = (self.id_rows.len() - 1) as u32;
                    self.identity.entry(node.id).or_default().push(index);
                }
                Seen::Missing => {}
                Seen::Unknown(why) => return Err(PolicyError::AnchorUnreadable { anchor, why }),
            }
        }
        Ok(())
    }

    fn verified_firmlinks(
        table: &Table,
        probe: &dyn Probe,
    ) -> Result<Option<DataVolume>, PolicyError> {
        let Some((root, links)) = table.data_volume else {
            return Ok(None);
        };
        let root = AbsPath::parse(root.as_ref()).map_err(|_| PolicyError::BadAnchor(root))?;
        let mut firmlinks = Vec::new();
        for &link in links {
            let spelled = |base: &str| {
                AbsPath::parse(format!("{base}/{link}").as_ref())
                    .map_err(|_| PolicyError::BadAnchor(link))
            };
            let direct = probe.node(&spelled("")?);
            let through = probe.node(&spelled(&root.as_path().to_string_lossy())?);
            if let (Seen::Found(a), Seen::Found(b)) = (direct, through)
                && a.id == b.id
            {
                firmlinks.push(if table.fold_case {
                    link.to_ascii_lowercase().into_bytes()
                } else {
                    link.as_bytes().to_vec()
                });
            }
        }
        firmlinks.sort_by_key(|f| std::cmp::Reverse(f.len()));
        Ok(Some(DataVolume { root, firmlinks }))
    }

    fn critical_by_spelling(&self, path: &AbsPath) -> Option<Kind> {
        let mut scratch = Vec::new();
        let names: Vec<&OsStr> = path.names().collect();
        let ruling = match names.split_last() {
            Some((leaf, dirs)) if path.as_path().parent().is_some() => {
                let mut step = self.origin(None, Bundles::Named);
                for name in dirs {
                    step = self.step(&step, name, None, &mut scratch);
                }
                self.rule(path, &step, leaf, None, &mut scratch)
            }
            _ => self.rule_root(path, None, Bundles::Named, &mut scratch),
        };
        ruling.critical.map(|hit| hit.kind)
    }

    /// The spelling every rule is matched against: a verified firmlink's data-volume spelling is
    /// mapped to the root spelling, longest firmlink first (SAFE-03, I-07). Others are unchanged.
    pub(crate) fn canonical<'p>(&self, path: &'p AbsPath) -> Cow<'p, AbsPath> {
        let Some(volume) = &self.data_volume else {
            return Cow::Borrowed(path);
        };
        let root = volume.root.bytes();
        let bytes = path.bytes();
        let same = |a: &[u8], b: &[u8]| {
            if self.fold_case {
                a.eq_ignore_ascii_case(b)
            } else {
                a == b
            }
        };
        if bytes.len() <= root.len()
            || bytes[root.len()] != SEP
            || !same(&bytes[..root.len()], root)
        {
            return Cow::Borrowed(path);
        }
        let rest = &bytes[root.len() + 1..];
        let mapped = volume.firmlinks.iter().any(|link| {
            rest.len() >= link.len()
                && same(&rest[..link.len()], link)
                && rest.get(link.len()).is_none_or(|&b| b == SEP)
        });
        if !mapped {
            return Cow::Borrowed(path);
        }
        let skip = volume.root.as_path().components().count();
        let mut out = PathBuf::from(MAIN_SEPARATOR.to_string());
        out.extend(path.as_path().components().skip(skip));
        Cow::Owned(AbsPath::trusted(out))
    }

    /// Whether a name read from a directory can still be an alias that folding misses. ASCII
    /// case is the only alias an all-ASCII name has on a case-insensitive volume, and folding
    /// covers it; a case fold or a normalization form needs a byte outside ASCII.
    pub(crate) fn may_be_alias(&self, path: &AbsPath) -> bool {
        self.fold_case && !path.bytes().is_ascii()
    }

    /// Whether names compare without ASCII case. [`Policy::may_be_alias`] covers the folds
    /// outside ASCII.
    pub(crate) fn folds_case(&self) -> bool {
        self.fold_case
    }

    fn trie(&self, which: Which) -> &Trie {
        match which {
            Which::Critical => &self.critical,
            Which::Protection => &self.protection,
        }
    }

    fn child_rows(&self, id: Option<FileId>) -> Vec<u32> {
        id.and_then(|id| self.identity.get(&id))
            .map(|rows| {
                rows.iter()
                    .copied()
                    .filter(|&r| self.id_rows[r as usize].scope == Scope::Children)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The step above every path. On Unix it is `/` itself, and `id` is the root's identity.
    /// Every step below it keeps its `bundles`.
    fn origin(&self, id: Option<FileId>, bundles: Bundles) -> Step {
        let position = |which| {
            let mut inherit = None;
            for &(scope, outcome) in &self.trie(which).nodes[ORIGIN as usize].rules {
                if matches!(scope, Scope::Subtree | Scope::Below) {
                    let won = Won {
                        depth: 0,
                        rank: outcome.rank(),
                        outcome,
                        via: Via::Row,
                    };
                    enter(&mut inherit, won);
                }
            }
            Position {
                nodes: vec![ORIGIN],
                inherit,
            }
        };
        Step {
            depth: 0,
            critical: position(Which::Critical),
            protection: position(Which::Protection),
            child_rows: self.child_rows(id),
            bundles,
        }
    }

    /// The step for a root folder: `/`, or a drive such as `C:\`.
    pub(crate) fn root_step(
        &self,
        root: &AbsPath,
        id: Option<FileId>,
        bundles: Bundles,
        scratch: &mut Vec<u8>,
    ) -> Step {
        match root.names().next() {
            None => self.origin(id, bundles),
            Some(drive) => self.step(&self.origin(None, bundles), drive, id, scratch),
        }
    }

    /// Steps every trie from `parent` into the directory `name`, and folds in identity hits for
    /// that directory: a Subtree anchor with the same `id` refuses everything below it.
    pub(crate) fn step(
        &self,
        parent: &Step,
        name: &OsStr,
        id: Option<FileId>,
        scratch: &mut Vec<u8>,
    ) -> Step {
        self.fold_into(name.as_encoded_bytes(), scratch);
        let mut critical = Position::default();
        let mut protection = Position::default();
        let (_, inherit) = self.look(
            Which::Critical,
            parent,
            scratch,
            id,
            Some(&mut critical.nodes),
            None,
        );
        critical.inherit = inherit;
        let (_, inherit) = self.look(
            Which::Protection,
            parent,
            scratch,
            id,
            Some(&mut protection.nodes),
            None,
        );
        protection.inherit = inherit;
        Step {
            depth: parent.depth + 1,
            critical,
            protection,
            child_rows: self.child_rows(id),
            bundles: parent.bundles,
        }
    }

    /// The contest for the entry `name` under `parent`, in one layer. Returns the winner for the
    /// entry itself and the winner its descendants inherit.
    fn look<'p>(
        &'p self,
        which: Which,
        parent: &Step,
        folded: &[u8],
        id: Option<FileId>,
        mut next: Option<&mut Vec<u32>>,
        mut inside: Option<&mut Vec<&'p Inside>>,
    ) -> (Option<Won>, Option<Won>) {
        let trie = self.trie(which);
        let position = match which {
            Which::Critical => &parent.critical,
            Which::Protection => &parent.protection,
        };
        let depth = parent.depth + 1;
        let row = |outcome: Outcome, via| Won {
            depth,
            rank: outcome.rank(),
            outcome,
            via,
        };
        let mut at = position.inherit;
        let mut inherit = position.inherit;
        let mut names_children_by_bundle = false;

        let mut visit = |node: &'p TrieNode, index: u32| {
            for &(scope, outcome) in &node.rules {
                let won = row(outcome, Via::Row);
                if matches!(scope, Scope::Exact | Scope::Subtree) {
                    enter(&mut at, won);
                }
                if matches!(scope, Scope::Subtree | Scope::Below) {
                    enter(&mut inherit, won);
                }
            }
            if let Some(next) = next.as_deref_mut() {
                next.push(index);
            }
            if let Some(inside) = inside.as_deref_mut() {
                inside.extend(&node.inside);
            }
        };
        for &node in &position.nodes {
            trie.each_child(node, folded, &mut visit);
        }
        trie.each_child(FLOATING, folded, &mut visit);
        // After the entry's own rows, so a row anchored on the entry beats one on its parent.
        for &node in &position.nodes {
            let node = &trie.nodes[node as usize];
            names_children_by_bundle |= node.bundle_parent;
            for &(scope, outcome) in &node.rules {
                if scope == Scope::Children {
                    enter(&mut at, row(outcome, Via::ChildRow));
                }
            }
        }

        let same_file = |r: u32| {
            let id_row = &self.id_rows[r as usize];
            (id_row.which == which).then(|| {
                (
                    id_row.scope,
                    row(Outcome::Refuse(id_row.kind), Via::SameFile(r)),
                )
            })
        };
        for &r in id
            .and_then(|id| self.identity.get(&id))
            .into_iter()
            .flatten()
        {
            if let Some((scope, won)) = same_file(r) {
                if matches!(scope, Scope::Exact | Scope::Subtree) {
                    enter(&mut at, won);
                }
                if matches!(scope, Scope::Subtree | Scope::Below) {
                    enter(&mut inherit, won);
                }
            }
        }
        for &r in &parent.child_rows {
            if let Some((_, won)) = same_file(r) {
                enter(&mut at, won);
            }
        }

        if which == Which::Protection {
            let name = |kind, via| Won {
                depth,
                rank: RANK_NAME,
                outcome: Outcome::Refuse(kind),
                via,
            };
            if let Some(kind) = self.names.keyword(folded) {
                enter(&mut at, name(kind, Via::Keyword));
                enter(&mut inherit, name(kind, Via::Keyword));
            }
            if parent.bundles == Bundles::Named && !bundles_off(position.inherit) {
                let won = name(Kind::ProtectedBundle, Via::Bundle);
                match self.names.bundle(folded) {
                    BundleMatch::No => {}
                    BundleMatch::Name => {
                        enter(&mut at, won);
                        if names_children_by_bundle {
                            enter(&mut inherit, won);
                        }
                    }
                    BundleMatch::Anywhere => {
                        enter(&mut at, won);
                        enter(&mut inherit, won);
                    }
                }
            }
        }
        (at, inherit)
    }

    /// Layers 2 and 4 for the leaf `path`, named `name` in the directory at `parent`.
    pub(crate) fn rule<'p>(
        &'p self,
        path: &AbsPath,
        parent: &Step,
        name: &OsStr,
        id: Option<FileId>,
        scratch: &mut Vec<u8>,
    ) -> Ruling<'p> {
        let mut inside = Vec::new();
        self.fold_into(name.as_encoded_bytes(), scratch);
        let (critical, _) = self.look(Which::Critical, parent, scratch, id, None, None);
        let (mut protection, _) = self.look(
            Which::Protection,
            parent,
            scratch,
            id,
            None,
            Some(&mut inside),
        );
        let depth = parent.depth + 1;
        if parent.bundles == Bundles::Named
            && !bundles_off(parent.protection.inherit)
            && self.names.whole_path(path.bytes())
        {
            let won = Won {
                depth,
                rank: RANK_NAME,
                outcome: Outcome::Refuse(Kind::ProtectedBundle),
                via: Via::Bundle,
            };
            enter(&mut protection, won);
        }
        Ruling {
            critical: critical.and_then(|won| self.hit(won, depth, path)),
            protection: protection.and_then(|won| self.hit(won, depth, path)),
            inside,
        }
    }

    /// Layers 2 and 4 for a root folder, which has no parent directory.
    pub(crate) fn rule_root<'p>(
        &'p self,
        root: &AbsPath,
        id: Option<FileId>,
        bundles: Bundles,
        scratch: &mut Vec<u8>,
    ) -> Ruling<'p> {
        match root.names().next() {
            Some(drive) => self.rule(root, &self.origin(None, bundles), drive, id, scratch),
            None => Ruling {
                critical: self
                    .at_origin(Which::Critical, id)
                    .and_then(|won| self.hit(won, 0, root)),
                protection: self
                    .at_origin(Which::Protection, id)
                    .and_then(|won| self.hit(won, 0, root)),
                inside: self.protection.nodes[ORIGIN as usize]
                    .inside
                    .iter()
                    .collect(),
            },
        }
    }

    fn at_origin(&self, which: Which, id: Option<FileId>) -> Option<Won> {
        let mut at = None;
        let won = |outcome: Outcome, via| Won {
            depth: 0,
            rank: outcome.rank(),
            outcome,
            via,
        };
        for &(scope, outcome) in &self.trie(which).nodes[ORIGIN as usize].rules {
            if matches!(scope, Scope::Exact | Scope::Subtree) {
                enter(&mut at, won(outcome, Via::Row));
            }
        }
        for &r in id
            .and_then(|id| self.identity.get(&id))
            .into_iter()
            .flatten()
        {
            let row = &self.id_rows[r as usize];
            if row.which == which && matches!(row.scope, Scope::Exact | Scope::Subtree) {
                enter(&mut at, won(Outcome::Refuse(row.kind), Via::SameFile(r)));
            }
        }
        at
    }

    fn hit(&self, won: Won, leaf_depth: u16, path: &AbsPath) -> Option<Hit> {
        let Outcome::Refuse(kind) = won.outcome else {
            return None;
        };
        let up = usize::from(leaf_depth - won.depth);
        let above = |levels: usize| match path.as_path().ancestors().nth(levels) {
            Some(prefix) => AbsPath::trusted(prefix.to_path_buf()),
            None => path.clone(),
        };
        let (anchor, how) = match won.via {
            Via::SameFile(r) => (self.id_rows[r as usize].anchor.clone(), How::SameFileAs),
            Via::ChildRow => (above(1), How::ChildOf),
            Via::Bundle if up == 0 => (path.clone(), How::BundleId),
            Via::Row | Via::Keyword if up == 0 => (path.clone(), How::Exact),
            Via::Row | Via::Keyword | Via::Bundle => (above(up), How::Inside),
        };
        Some(Hit { kind, anchor, how })
    }

    /// Layer 3's pure part for `path`: PowerLog, EDR, user-cache scope owner, in-use family.
    pub(crate) fn live<'p>(&self, path: &'p AbsPath, scratch: &mut Vec<u8>) -> LiveChecks<'p> {
        self.live.check(path, scratch)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::super::table::Anchor::Abs;
    use super::super::table::Effect::{Allow, Refuse, RefuseWithAncestors};
    use super::super::table::Scope::{Below, Exact, Subtree};
    use super::super::table::{Live, Names, row};
    use super::*;
    use crate::guard::{Guard, Homes, KeptReason, Verdict};
    use crate::paths::Os;
    use crate::probe::fake::FakeProbe;
    use crate::whitelist::Whitelist;

    static STATE: Table = Table {
        critical: &[row(Abs("/"), Exact, Refuse(Kind::SystemFolder))],
        protection: &[row(
            Abs("/data/apps/engine/state"),
            Subtree,
            RefuseWithAncestors(Kind::AppState("engine state")),
        )],
        carve_outs: &[row(Abs("/data"), Below, Allow)],
        names: Names {
            bundle_globs: &[],
            bundle_parents: &[],
            keywords: &[],
        },
        live: Live {
            user_cache_scopes: &[],
            group_cache_scopes: &[],
            powerlog_stores: None,
            edr: None,
            sqlite_suffixes: &[],
            partial_downloads: &[],
        },
        data_volume: None,
        fold_case: false,
    };

    fn judge(fs: &FakeProbe, path: &str) -> Verdict {
        let env = |_: &str| None;
        let home = AbsPath::parse("/home/a".as_ref()).unwrap();
        let inputs = GuardInputs {
            os: Os::current(),
            homes: Homes::new(home.clone(), None),
            env: &env,
            whitelist: Whitelist::defaults(Os::current(), &home),
        };
        let guard = Guard::from_policy(Policy::from_table(&STATE, &inputs, fs).unwrap());
        let path = AbsPath::parse(path.as_ref()).unwrap();
        guard.pass(fs).evaluate(path).verdict().clone()
    }

    #[test]
    fn a_folder_that_contains_protected_state_is_kept_while_the_state_exists() {
        let with_state = FakeProbe::new()
            .dir("/data/apps/engine/state")
            .dir("/data/apps/other");
        let kept = judge(&with_state, "/data/apps/engine");
        assert!(
            matches!(&kept, Verdict::Kept(r @ KeptReason::Protected(hit))
                if r.code() == "contains-protected"
                    && hit.how == How::Contains { inner: AbsPath::parse("/data/apps/engine/state".as_ref()).unwrap() }),
            "{kept:?}"
        );
        assert_eq!(
            kept.to_string(),
            "Kept · engine state (contains /data/apps/engine/state)"
        );
        assert!(matches!(judge(&with_state, "/data/apps"), Verdict::Kept(_)));
        assert_eq!(judge(&with_state, "/data/apps/other"), Verdict::Eligible);

        let without = FakeProbe::new().dir("/data/apps/engine");
        assert_eq!(judge(&without, "/data/apps/engine"), Verdict::Eligible);
        assert_eq!(judge(&without, "/data/apps"), Verdict::Eligible);
    }
}
