//! Linux policy (`linux.md` rule R2 and row LNX-ANA-05). Cites are to Mole's `linux` branch at
//! 4de579cc: `mole_is_protected_path`, lib/core/delete.sh:20-37, analyze's list,
//! cmd/analyze/delete.go:116-173, and the never-touched list in AGENTS.md:29.
//!
//! Rows marked inferred have no Mole source. Mole's Linux clean works from an allowlist of
//! roots, so it never needed them; dustpan judges any path, so it does. M9 confirms them.
use super::table::Anchor::{self, Abs, Env, Home};
use super::table::Effect::{Allow, Refuse};
use super::table::Scope::{Below, Children, Exact, Subtree};
use super::table::{Live, Names, Row, Table, row};
use crate::guard::Kind::{
    self, HomeFolder, HomeRoot, SharedRoot, SystemFolder, SystemPath, UserData,
};

pub(super) static TABLE: Table = Table {
    critical: CRITICAL,
    protection: PROTECTION,
    carve_outs: &[],
    names: Names {
        bundle_globs: &[],
        bundle_parents: &[],
        keywords: &[],
    },
    live: LIVE,
    data_volume: None,
    fold_case: false,
};

const fn root(at: &'static str) -> Row {
    row(Abs(at), Exact, Refuse(SystemFolder))
}

const fn system_tree(at: &'static str) -> Row {
    row(Abs(at), Subtree, Refuse(SystemPath))
}

const fn tree(at: Anchor, kind: Kind) -> Row {
    row(at, Subtree, Refuse(kind))
}

const fn only(at: Anchor, kind: Kind) -> Row {
    row(at, Exact, Refuse(kind))
}

static CRITICAL: &[Row] = &[
    root("/"),
    system_tree("/bin"),
    system_tree("/boot"),
    system_tree("/dev"),
    system_tree("/etc"),
    system_tree("/lib"),
    system_tree("/lib64"),
    system_tree("/proc"),
    system_tree("/root"),
    system_tree("/run"),
    system_tree("/sbin"),
    system_tree("/sys"),
    system_tree("/usr"),
    system_tree("/var"),
    system_tree("/nix/store"),
    system_tree("/snap"),
    only(Home(""), HomeRoot),
    // Inferred: other account roots, shared mount and scratch roots, and the XDG user folders,
    // by analogy with the macOS and Windows head folders.
    root("/home"),
    row(Abs("/home"), Children, Refuse(HomeRoot)),
    // Inferred: Fedora Atomic keeps homes under `/var/home`, with `/home` a link to it.
    row(Abs("/var/home"), Below, Allow),
    row(Abs("/var/home"), Children, Refuse(HomeRoot)),
    root("/tmp"),
    root("/opt"),
    root("/srv"),
    root("/mnt"),
    root("/media"),
    only(Home("Desktop"), HomeFolder),
    only(Home("Documents"), HomeFolder),
    only(Home("Downloads"), HomeFolder),
    only(Home("Music"), HomeFolder),
    only(Home("Pictures"), HomeFolder),
    only(Home("Videos"), HomeFolder),
];

const KEYS: Kind = UserData("keys and credentials");
const SESSION: Kind = UserData("desktop session settings");
const MODELS: Kind = UserData("downloaded models and browsers");

static PROTECTION: &[Row] = &[
    only(Home(".local"), SharedRoot),
    tree(Home(".ssh"), KEYS),
    tree(Home(".gnupg"), KEYS),
    tree(Home(".local/share/keyrings"), KEYS),
    tree(Home(".cache/doc"), UserData("document portal")),
    only(Home(".local/share/applications"), SESSION),
    only(Home(".config/systemd"), SESSION),
    only(Home(".config/systemd/user"), SESSION),
    only(Home(".config/autostart"), SESSION),
    only(Home(".var"), SharedRoot),
    only(Home(".var/app"), SharedRoot),
    tree(Home(".ollama"), MODELS),
    tree(Home(".cache/huggingface"), MODELS),
    tree(Home(".cache/lm-studio"), MODELS),
    tree(Home(".cache/ms-playwright"), MODELS),
    tree(Home(".cache/Cypress"), MODELS),
    tree(Home(".cache/puppeteer"), MODELS),
    // Inferred: Mole protects its own folders under each XDG base, and its rule roots may not
    // be an ancestor of a base. The bases themselves are kept here.
    only(Home(".cache"), SharedRoot),
    only(Home(".config"), SharedRoot),
    only(Home(".local/share"), SharedRoot),
    only(Home(".local/state"), SharedRoot),
    only(Env("XDG_CACHE_HOME", ""), SharedRoot),
    only(Env("XDG_CONFIG_HOME", ""), SharedRoot),
    only(Env("XDG_DATA_HOME", ""), SharedRoot),
    only(Env("XDG_STATE_HOME", ""), SharedRoot),
];

/// Inferred from the macOS rules: Mole's Linux branch probes owners per rule, not per path.
const LIVE: Live = Live {
    user_cache_scopes: &[],
    group_cache_scopes: &[],
    powerlog_stores: None,
    edr: None,
    sqlite_suffixes: &[".db", ".sqlite", ".sqlite3"],
    partial_downloads: &[("Downloads", &[".crdownload", ".part"])],
};
