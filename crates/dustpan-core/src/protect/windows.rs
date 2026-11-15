//! Windows policy (`safety-invariants.md` section 2.9, `windows.md` sections 3.5 and 3.6). The
//! single source for what Windows protects. Cites are to Mole's `windows` branch at 59f3dc5a.
//! Compiles on every host from M0; its rules run in tests on the Windows CI runners.
//!
//! Clean's extra segments (lib/core/base.ps1:214-248), the `%LOCALAPPDATA%\Packages` rule and
//! the OneDrive roots arrive with the Windows catalog (M8).
use super::table::Anchor::{self, Abs, Env, Home, Segments};
use super::table::Effect::{Refuse, RefuseWithAncestors};
use super::table::Scope::{Children, Exact, Subtree};
use super::table::{Live, Names, Row, Table, row};
use crate::guard::Kind::{
    self, AppState, HomeFolder, HomeRoot, SystemFolder, SystemPath, UserData,
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
    fold_case: true,
};

const fn system_tree(at: Anchor) -> Row {
    row(at, Subtree, Refuse(SystemPath))
}

const fn only(at: Anchor, kind: Kind) -> Row {
    row(at, Exact, Refuse(kind))
}

/// lib/core/base.ps1:184-199 and cmd/analyze/protect_windows.go:13-167. Literal `C:` rows stand
/// beside env rows, so an unset or redirected variable cannot remove `C:\Windows`.
static CRITICAL: &[Row] = &[
    // `System32` and `SysWOW64` sit under `C:\Windows`, and the Program Files Defender folders
    // under Program Files.
    system_tree(Abs(r"C:\Windows")),
    system_tree(Abs(r"C:\Program Files")),
    system_tree(Abs(r"C:\Program Files (x86)")),
    system_tree(Abs(r"C:\ProgramData\Microsoft\Windows Defender")),
    system_tree(Env("SystemRoot", "")),
    system_tree(Env("windir", "")),
    system_tree(Env("ProgramFiles", "")),
    system_tree(Env("ProgramFiles(x86)", "")),
    system_tree(Env("ProgramData", r"\Microsoft\Windows Defender")),
    // Analyze: the system drive, everything at its first level, and these trees.
    only(Abs(r"C:\"), SystemFolder),
    row(Abs(r"C:\"), Children, Refuse(SystemFolder)),
    system_tree(Abs(r"C:\ProgramData")),
    system_tree(Abs(r"C:\Users\Default")),
    system_tree(Abs(r"C:\Users\Public")),
    system_tree(Abs(r"C:\Recovery")),
    system_tree(Abs(r"C:\System Volume Information")),
    system_tree(Abs(r"C:\$Recycle.Bin")),
    system_tree(Abs(r"C:\Config.Msi")),
    system_tree(Abs(r"C:\PerfLogs")),
    system_tree(Abs(r"C:\Boot")),
    system_tree(Abs(r"C:\Documents and Settings")),
    system_tree(Abs(r"C:\$WinREAgent")),
    row(Abs(r"C:\Users"), Children, Refuse(HomeRoot)),
    only(Env("SystemDrive", r"\"), SystemFolder),
    row(Env("SystemDrive", r"\"), Children, Refuse(SystemFolder)),
    system_tree(Env("SystemDrive", r"\ProgramData")),
    system_tree(Env("SystemDrive", r"\Users\Default")),
    system_tree(Env("SystemDrive", r"\Users\Public")),
    system_tree(Env("SystemDrive", r"\Recovery")),
    system_tree(Env("SystemDrive", r"\System Volume Information")),
    system_tree(Env("SystemDrive", r"\$Recycle.Bin")),
    system_tree(Env("SystemDrive", r"\Config.Msi")),
    system_tree(Env("SystemDrive", r"\PerfLogs")),
    system_tree(Env("SystemDrive", r"\Boot")),
    system_tree(Env("SystemDrive", r"\Documents and Settings")),
    system_tree(Env("SystemDrive", r"\$WinREAgent")),
    row(Env("SystemDrive", r"\Users"), Children, Refuse(HomeRoot)),
    // The profile root and its head folders; their contents stay deletable.
    only(Home(""), HomeRoot),
    only(Home("Desktop"), HomeFolder),
    only(Home("Documents"), HomeFolder),
    only(Home("Downloads"), HomeFolder),
    only(Home("Pictures"), HomeFolder),
    only(Home("Videos"), HomeFolder),
    only(Home("Music"), HomeFolder),
    only(Home("AppData"), HomeFolder),
    only(Home(r"AppData\Local"), HomeFolder),
    only(Home(r"AppData\Roaming"), HomeFolder),
    only(Home(r"AppData\LocalLow"), HomeFolder),
];

const CREDENTIALS: Kind = UserData("Windows credentials");

/// lib/core/base.ps1:205-212: whole segments, wherever they appear.
static PROTECTION: &[Row] = &[
    row(
        Segments(&["Microsoft", "Credentials"]),
        Subtree,
        Refuse(CREDENTIALS),
    ),
    row(
        Segments(&["Microsoft", "Protect"]),
        Subtree,
        Refuse(CREDENTIALS),
    ),
    row(
        Segments(&["Microsoft", "Crypto"]),
        Subtree,
        Refuse(CREDENTIALS),
    ),
    row(
        Segments(&["Microsoft", "Vault"]),
        Subtree,
        Refuse(CREDENTIALS),
    ),
    row(
        Segments(&["Microsoft", "SystemCertificates"]),
        Subtree,
        Refuse(UserData("Windows certificates")),
    ),
    row(
        Segments(&["Microsoft", "Windows", "Start Menu"]),
        Subtree,
        Refuse(UserData("Start menu")),
    ),
    row(
        Env("LOCALAPPDATA", r"\Docker"),
        Subtree,
        RefuseWithAncestors(AppState("Docker Desktop state")),
    ),
];

const LIVE: Live = Live {
    user_cache_scopes: &[],
    powerlog_stores: None,
    edr: None,
    sqlite_suffixes: &[".db", ".sqlite", ".sqlite3"],
    partial_downloads: &[("Downloads", &[".crdownload", ".part", ".partial"])],
};
