//! macOS policy. The single source for what macOS protects. Cites are to `../mole` at 5dd93fe3.
//!
//! Mole keeps two lists: the shell validator that `clean`, `purge` and `uninstall` share, and
//! analyze's own Go list. Dustpan has one guard, so this table is their union.
use super::macos_bundles::{DATA_PROTECTED_BUNDLES, SYSTEM_CRITICAL_BUNDLES};
use super::table::Anchor::{self, Abs, Home, Segments};
use super::table::Effect::{Allow, AllowBundles, Refuse, RefuseWithAncestors};
use super::table::Scope::{Below, Children, Exact, Subtree};
use super::table::{Live, Names, Row, Table, row};
use crate::guard::Kind::{
    self, AppState, CloudStructure, HomeFolder, HomeRoot, SharedRoot, StructuralFolder,
    SystemFolder, SystemPath, UserData,
};

pub(super) static TABLE: Table = Table {
    critical: CRITICAL,
    protection: PROTECTION,
    carve_outs: CARVE_OUTS,
    names: Names {
        bundle_globs: &[
            INLINE_BUNDLE_FAMILIES,
            SYSTEM_CRITICAL_BUNDLES,
            DATA_PROTECTED_BUNDLES,
        ],
        bundle_parents: &[&["Library", "Containers"], &["Library", "Group Containers"]],
        keywords: KEYWORDS,
    },
    live: LIVE,
    data_volume: Some(("/System/Volumes/Data", FIRMLINKS)),
    fold_case: true,
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

/// `_mole_is_critical_deletion_path`, lib/core/file_ops.sh:1044-1129, plus analyze's structural
/// list, cmd/analyze/delete.go:322-560 (`mac-analyze-status-tui.md` section 2.8, step 5).
static CRITICAL: &[Row] = &[
    // Children stay deletable by this layer; the roots themselves fall to the rows below.
    row(Abs("/usr/local"), Below, Allow),
    row(Abs("/opt/homebrew"), Below, Allow),
    // Exact roots: refused themselves, children judged by later rows and layers.
    root("/"),
    root("/Applications"),
    root("/Library"),
    root("/Library/Application Support"),
    root("/Volumes"),
    root("/Network"),
    root("/cores"),
    root("/home"),
    root("/net"),
    root("/opt"),
    root("/opt/homebrew"),
    root("/Users"),
    root("/Users/Shared"),
    root("/private"),
    root("/private/tmp"),
    root("/private/var"),
    root("/private/var/tmp"),
    root("/private/var/folders"),
    root("/tmp"),
    root("/var"),
    root("/var/root"),
    // Trees: the root and every descendant.
    system_tree("/bin"),
    system_tree("/dev"),
    system_tree("/sbin"),
    system_tree("/usr"),
    system_tree("/System"),
    system_tree("/etc"),
    system_tree("/private/etc"),
    system_tree("/var/db"),
    system_tree("/var/audit"),
    system_tree("/private/var/db"),
    system_tree("/private/var/audit"),
    system_tree("/private/var/root"),
    system_tree("/Library/Apple"),
    system_tree("/Library/Extensions"),
    system_tree("/Library/Keychains"),
    system_tree("/Applications/Finder.app"),
    system_tree("/Applications/Safari.app"),
    system_tree("/Users/Guest"),
    // Read-only surfaces: age and process lists cannot prove them idle (SAFE-37).
    system_tree("/Library/Updates"),
    system_tree("/macOS Install Data"),
    // Every account root, and analyze's structural folders.
    row(Abs("/Users"), Children, Refuse(HomeRoot)),
    row(Abs("/Library"), Children, Refuse(StructuralFolder)),
    only(Home(""), HomeRoot),
    only(Home("Library"), HomeFolder),
    row(Home("Library"), Children, Refuse(StructuralFolder)),
    only(Home("Documents"), HomeFolder),
    only(Home("Desktop"), HomeFolder),
    only(Home("Downloads"), HomeFolder),
    only(Home("Pictures"), HomeFolder),
    only(Home("Movies"), HomeFolder),
    only(Home("Music"), HomeFolder),
    // Darwin per-user cache structure: deeper than the carve-out below, so it wins
    // (analyze's `isEndpointSecurityCachePath`, delete.go:655-673). Leaves under it stay eligible.
    only(Abs("/private/var/folders/*"), StructuralFolder),
    only(Abs("/private/var/folders/*/*"), StructuralFolder),
    only(Abs("/private/var/folders/*/*/*"), StructuralFolder),
];

/// `validate_path_for_deletion`'s early accepts, lib/core/file_ops.sh:1276-1281, 1351-1364.
static CARVE_OUTS: &[Row] = &[
    row(
        Abs("/System/Library/Caches/com.apple.coresymbolicationd/data"),
        Subtree,
        Allow,
    ),
    row(Abs("/private/tmp"), Below, Allow),
    row(Abs("/private/var/tmp"), Below, Allow),
    row(Abs("/private/var/folders"), Below, Allow),
    row(Abs("/private/var/log"), Subtree, Allow),
    row(Abs("/private/var/db/diagnostics"), Subtree, Allow),
    row(Abs("/private/var/db/DiagnosticPipeline"), Subtree, Allow),
    row(Abs("/private/var/db/powerlog"), Subtree, Allow),
    row(Abs("/private/var/db/reportmemoryexception"), Subtree, Allow),
    row(Abs("/private/var/db/receipts/*.bom"), Exact, Allow),
    row(Abs("/private/var/db/receipts/*.plist"), Exact, Allow),
];

const DOCKER: Kind = AppState("Docker Desktop state");
const ORBSTACK: Kind = AppState("OrbStack state");
const CODEX: Kind = UserData("Codex sessions and state");
const CLAUDE: Kind = UserData("Claude Code transcripts");
const AUDIO_PLUGINS: Kind = UserData("audio plug-ins and licences");
const LICENCE: Kind = UserData("licence state");
const BROWSER_BINARIES: Kind = UserData("installed browser binaries");
const SYSTEM_SERVICE: Kind = AppState("system service state");

/// `should_protect_path`, lib/core/app_protection.sh:406-695, and analyze's app-state rows
/// (cmd/analyze/delete.go:356-404, 512-560). A `Segments` row is Mole's `*/a/b` glob. Rows that
/// a keyword or a bundle glob already covers are not repeated. Steam's library is protected in
/// uninstall mode only, and that row arrives with uninstall (M5).
static PROTECTION: &[Row] = &[
    // A direct child of the user's Trash skips this layer: emptying Trash is an explicit discard.
    row(Home(".Trash"), Children, Allow),
    // Container cache and temp folders are rebuildable even when the container id is protected.
    row(
        Segments(&["Library", "Containers", "*", "Data", "Library", "Caches"]),
        Below,
        AllowBundles,
    ),
    row(
        Segments(&["Library", "Containers", "*", "Data", "tmp"]),
        Below,
        AllowBundles,
    ),
    row(
        Segments(&[
            "Library",
            "Group Containers",
            "*",
            "Data",
            "Library",
            "Caches",
        ]),
        Below,
        AllowBundles,
    ),
    row(
        Segments(&["Library", "Group Containers", "*", "Data", "tmp"]),
        Below,
        AllowBundles,
    ),
    row(
        Home("Library/Containers/com.tencent.xinWeChat/Data/Documents/app_data/log"),
        Below,
        AllowBundles,
    ),
    row(
        Home("Library/Containers/com.tencent.xinWeChat/Data/.wxapplet/WMPF"),
        Below,
        AllowBundles,
    ),
    // Codex Desktop's Chromium cache leaves (app_protection.sh:436-448).
    row(
        Home("Library/Caches/Codex/Default/Cache"),
        Below,
        AllowBundles,
    ),
    row(
        Home("Library/Caches/Codex/Default/Code Cache"),
        Below,
        AllowBundles,
    ),
    row(
        Home("Library/Caches/Codex/Default/Partitions/codex-browser-app/Cache"),
        Below,
        AllowBundles,
    ),
    row(
        Home("Library/Caches/Codex/Default/Partitions/codex-browser-app/Code Cache"),
        Below,
        AllowBundles,
    ),
    row(
        Home("Library/Caches/Codex/codex-browser-app/Cache"),
        Below,
        AllowBundles,
    ),
    row(
        Home("Library/Caches/Codex/codex-browser-app/Code Cache"),
        Below,
        AllowBundles,
    ),
    // Clean-only state (410-423).
    tree(Home(".prometheus/data"), UserData("Prometheus data")),
    tree(Home(".jupyter/runtime"), UserData("Jupyter runtime state")),
    tree(Home(".yarn/berry/cache"), UserData("Yarn Berry cache")),
    only(Home(".gitconfig.lock"), UserData("Git config lock")),
    tree(Home(".gitconfig.bak*"), UserData("Git config backup")),
    tree(
        Home(".config/fish/fish_history.bak*"),
        UserData("shell history backup"),
    ),
    tree(Home(".bash_history.bak*"), UserData("shell history backup")),
    tree(Home(".zsh_history.bak*"), UserData("shell history backup")),
    // Shared home roots (347-362, 425-427).
    only(Home(".cache"), SharedRoot),
    only(Home(".config"), SharedRoot),
    only(Home(".local"), SharedRoot),
    only(Home(".local/bin"), SharedRoot),
    only(Home(".local/lib"), SharedRoot),
    only(Home(".local/share"), SharedRoot),
    only(Home(".local/state"), SharedRoot),
    only(Home("Library/Unity"), SharedRoot),
    only(Home("Library/Application Support/Epic"), SharedRoot),
    // Container runtimes, with every ancestor that holds an existing store.
    row(
        Home("Library/Containers/com.docker.docker"),
        Subtree,
        RefuseWithAncestors(DOCKER),
    ),
    row(Home(".orbstack"), Subtree, RefuseWithAncestors(ORBSTACK)),
    row(
        Home("Library/Group Containers/*dev.orbstack*"),
        Subtree,
        RefuseWithAncestors(ORBSTACK),
    ),
    tree(
        Segments(&["Library", "Group Containers", "*dev.orbstack"]),
        ORBSTACK,
    ),
    tree(Segments(&[".orbstack"]), ORBSTACK),
    // Named user data (527-655).
    tree(Home("Library/Logs/dustpan"), UserData("dustpan's own logs")),
    tree(
        Segments(&["Library", "Application Support", "Codex"]),
        CODEX,
    ),
    row(
        Segments(&[
            "Library",
            "Application Support",
            "Codex",
            "Crashpad",
            "pending",
        ]),
        Children,
        Allow,
    ),
    tree(
        Segments(&[
            "Library",
            "Application Support",
            "Codex",
            "Crashpad",
            "pending",
            "*",
            "*",
        ]),
        CODEX,
    ),
    tree(Segments(&["Library", "Logs", "com.openai.codex"]), CODEX),
    tree(Segments(&[".codex", "sessions"]), CODEX),
    only(Segments(&[".codex", "auth.json"]), CODEX),
    only(Segments(&[".codex", "history.jsonl"]), CODEX),
    only(Segments(&[".codex", "state_*.sqlite"]), CODEX),
    only(Segments(&[".codex", "logs_*.sqlite"]), CODEX),
    only(Segments(&[".codex", "session_index.jsonl"]), CODEX),
    only(Segments(&[".codex", "cache", "session_index.jsonl"]), CODEX),
    tree(Segments(&[".codex", "cache", "codex_app_directory"]), CODEX),
    only(Home(".claude"), CLAUDE),
    tree(Segments(&[".claude", "projects"]), CLAUDE),
    tree(Segments(&[".claude", "file-history"]), CLAUDE),
    only(Segments(&[".claude", "history.jsonl"]), CLAUDE),
    only(
        Segments(&["ByHost", "com.apple.bluetooth.*"]),
        UserData("Bluetooth settings"),
    ),
    only(
        Segments(&["ByHost", "com.apple.wifi.*"]),
        UserData("Wi-Fi settings"),
    ),
    only(
        Segments(&[
            "Library",
            "Preferences",
            "com.apple.networkextension*.plist",
        ]),
        UserData("VPN settings"),
    ),
    tree(Segments(&["Mobile Documents*"]), CloudStructure),
    tree(Segments(&["Library", "Accounts"]), UserData("account data")),
    tree(Segments(&["Library", "Keychains"]), UserData("keychains")),
    tree(Segments(&["Library", "Mail"]), UserData("Mail data")),
    tree(
        Segments(&["Library", "Calendars"]),
        UserData("Calendar data"),
    ),
    tree(
        Segments(&["Library", "Contacts"]),
        UserData("Contacts data"),
    ),
    tree(Abs("/Library/Audio/Plug-Ins/Components"), AUDIO_PLUGINS),
    tree(Abs("/Library/Audio/Plug-Ins/VST"), AUDIO_PLUGINS),
    tree(Abs("/Library/Audio/Plug-Ins/VST3"), AUDIO_PLUGINS),
    tree(
        Segments(&["Library", "Application Support", "iZotope"]),
        AUDIO_PLUGINS,
    ),
    tree(
        Abs("/Library/Application Support/LaserSoft Imaging"),
        LICENCE,
    ),
    tree(
        Segments(&["Library", "Preferences", "com.native-instruments*"]),
        LICENCE,
    ),
    only(
        Segments(&["Library", "Preferences", "com.avid.mediacomposer*.plist"]),
        LICENCE,
    ),
    // Mole limits this one to a one- or two-digit version; any version is kept here.
    only(
        Segments(&["Library", "Preferences", "com.fabfilter.*.plist"]),
        LICENCE,
    ),
    only(
        Segments(&["Library", "Preferences", "com.paceap.*.plist"]),
        LICENCE,
    ),
    tree(
        Abs("/private/var/folders/*/*/C/com.native-instruments*"),
        LICENCE,
    ),
    tree(
        Abs("/private/var/folders/*/*/C/com.avid.mediacomposer*"),
        LICENCE,
    ),
    tree(
        Abs("/private/var/folders/*/*/C/com.paceap.eden.iLokLicenseManager*"),
        LICENCE,
    ),
    tree(
        Segments(&["Library", "Caches", "ms-playwright"]),
        BROWSER_BINARIES,
    ),
    tree(
        Segments(&["Library", "Caches", "camoufox"]),
        BROWSER_BINARIES,
    ),
    tree(
        Segments(&["Library", "Caches", "Cypress"]),
        BROWSER_BINARIES,
    ),
    tree(Segments(&[".cache", "puppeteer"]), BROWSER_BINARIES),
    tree(
        Segments(&["Library", "Caches", "app.cotypist.Cotypist"]),
        UserData("Cotypist models"),
    ),
    tree(
        Segments(&["Library", "Caches", "com.displaylink.DisplayLinkUserAgent"]),
        SYSTEM_SERVICE,
    ),
    tree(
        Segments(&["Library", "Caches", "com.lasersoft-imaging.SilverFast*"]),
        LICENCE,
    ),
    tree(Segments(&["Library", "Caches", "Adobe *"]), LICENCE),
    tree(Segments(&["Library", "Caches", "* Adobe*"]), LICENCE),
    tree(
        Segments(&["Library", "Caches", "com.apple.containermanagerd"]),
        SYSTEM_SERVICE,
    ),
    tree(
        Segments(&["Library", "Caches", "com.apple.homed"]),
        SYSTEM_SERVICE,
    ),
    tree(
        Segments(&["Library", "Caches", "com.apple.ap.adprivacyd"]),
        SYSTEM_SERVICE,
    ),
    tree(
        Segments(&["Library", "Caches", "FamilyCircle"]),
        SYSTEM_SERVICE,
    ),
    tree(
        Segments(&["Library", "Caches", "com.apple.HomeKit"]),
        SYSTEM_SERVICE,
    ),
    tree(
        Segments(&["Library", "Caches", "com.apple.*.ShortcutsSandboxCache"]),
        SYSTEM_SERVICE,
    ),
    // Deleting these while Gemini runs starts an unbounded write loop (Mole #1607).
    tree(
        Segments(&["Library", "Caches", "GIPPseudonymousID"]),
        SYSTEM_SERVICE,
    ),
    tree(
        Segments(&["Library", "Caches", "CCTClearcutLogger"]),
        SYSTEM_SERVICE,
    ),
    tree(
        Segments(&["Library", "Application Support", "com.apple.idleassetsd"]),
        UserData("wallpaper and screen saver assets"),
    ),
    tree(
        Segments(&["Library", "Application Support", "com.apple.wallpaper"]),
        UserData("wallpaper and screen saver assets"),
    ),
];

const SETTINGS: Kind = AppState("System Settings state");
const AUDIO: Kind = AppState("audio system state");

/// `should_protect_path` steps 1, 2, 4 and the CoreAudio arm (454-493, 513-518, 648-651): `*x*`
/// globs over the whole path. Mole spells some case-insensitively; all are folded here.
static KEYWORDS: &[(&str, Kind)] = &[
    ("SystemSettings", SETTINGS),
    ("SystemPreferences", SETTINGS),
    ("com.apple.Settings", SETTINGS),
    ("ControlCenter", AppState("Control Center state")),
    ("com.apple.Notes", UserData("Notes data")),
    ("com.apple.finder", AppState("Finder state")),
    ("com.apple.dock", AppState("Dock state")),
    ("com.apple.coreaudio", AUDIO),
    ("com.apple.audio.", AUDIO),
    ("coreaudiod", AUDIO),
];

/// `should_protect_data`'s inline families, lib/core/app_protection.sh:199-258. The two
/// generated lists hold the rest. Case-sensitive, as in Mole.
static INLINE_BUNDLE_FAMILIES: &[&str] = &[
    "com.apple.*",
    "loginwindow",
    "dock",
    "systempreferences",
    "finder",
    "safari",
    "org.cups.*",
    "backgroundtaskmanagement*",
    "keychain*",
    "security*",
    "bluetooth*",
    "wifi*",
    "network*",
    "tcc",
    "notification*",
    "accessibility*",
    "universalaccess*",
    "HIToolbox*",
    "*inputmethod*",
    "*InputMethod*",
    "*IME",
    "textinput*",
    "TextInput*",
    "keyboard*",
    "Keyboard*",
    "inputsource*",
    "InputSource*",
    "keylayout*",
    "KeyLayout*",
    "GlobalPreferences",
    ".GlobalPreferences",
    "org.pqrs.Karabiner*",
    "com.1password.*",
    "com.agilebits.*",
    "com.lastpass.*",
    "com.dashlane.*",
    "com.bitwarden.*",
    "com.jetbrains.*",
    "JetBrains*",
    "com.microsoft.*",
    "com.visualstudio.*",
    "com.sublimetext.*",
    "com.sublimehq.*",
    "Cursor",
    "Claude",
    "ChatGPT",
    "com.openai.codex",
    "Codex",
    "codex-runtimes",
    "Ollama",
    "com.nssurge.*",
    "com.v2ray.*",
    "com.clash.*",
    "ClashX*",
    "Surge*",
    "Shadowrocket*",
    "Quantumult*",
    "clash-*",
    "Clash-*",
    "*-clash",
    "*-Clash",
    "clash.*",
    "Clash.*",
    "clash_*",
    "*clash-verge*",
    "*Clash-Verge*",
    "clashverge*",
    "ClashVerge*",
    "com.docker.*",
    "com.getpostman.*",
    "com.insomnia.*",
];

const LIVE: Live = Live {
    user_cache_scopes: &[
        "Library/Caches/*",
        "Library/Containers/*/Data/Library/Caches",
        "Library/Group Containers/*/Caches",
        "Library/Group Containers/*/Library/Caches",
    ],
    powerlog_stores: Some("/private/var/db/powerlog/Library/PerfPowerTelemetry"),
    // `ENDPOINT_SECURITY_BUNDLE_PREFIXES`, lib/core/app_protection_data.sh:215-226.
    edr: Some((
        "/private/var/folders",
        &[
            "com.crowdstrike.",
            "com.sentinelone.",
            "com.sentinel-labs.",
            "com.eset.",
            "com.jamf.",
            "com.jamfsoftware.",
            "com.paloaltonetworks.",
            "com.cisco.anyconnect",
            "com.cisco.secureclient",
            "com.alibaba.security.",
        ],
    )),
    sqlite_suffixes: &[".db", ".sqlite", ".sqlite3"],
    partial_downloads: &[
        ("Downloads", &[".download", ".crdownload", ".part"]),
        ("Library/Caches/Homebrew/downloads", &[".incomplete"]),
    ],
};

/// `/usr/share/firmlinks` on macOS 15.
static FIRMLINKS: &[&str] = &[
    "AppleInternal",
    "Applications",
    "Library",
    "System/Library/Caches",
    "System/Library/Assets",
    "System/Library/PreinstalledAssets",
    "System/Library/AssetsV2",
    "System/Library/PreinstalledAssetsV2",
    "System/Library/CoreServices/CoreTypes.bundle/Contents/Library",
    "System/Library/Speech",
    "Users",
    "Volumes",
    "cores",
    "opt",
    "pkg",
    "private",
    "usr/local",
    "usr/libexec/cups",
    "usr/share/snmp",
];
