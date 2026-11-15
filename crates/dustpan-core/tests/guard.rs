//! Guard layers 1 to 4 against an in-memory file system: one case per path in
//! `docs/inventory/safety-invariants.md` sections 2.1 to 2.4, plus case and symlink aliases.
//! The macOS table runs on every Unix host, because it is selected by value.
#![cfg(unix)]

use dustpan_core::candidate::{GroupId, Rule, RuleId, Sink};
use dustpan_core::guard::{
    Guard, GuardInputs, How, KeptReason, Kind, Layer, LiveReason, PolicyError, ShapeReason, Verdict,
};
use dustpan_core::path::AbsPath;
use dustpan_core::paths::Os;
use dustpan_core::probe::fake::FakeProbe;
use dustpan_core::probe::{Owner, Probe, Seen, Unseen};
use std::sync::Arc;

const HOME: &str = "/Users/alice";

fn p(s: &str) -> AbsPath {
    AbsPath::parse(s.as_ref()).unwrap()
}

/// A macOS host: `/var`, `/tmp` and `/etc` are links into `/private`, case-insensitive APFS
/// makes `/SYSTEM` the same folder as `/System`, and the Data volume holds `/Users`.
fn mac() -> FakeProbe {
    FakeProbe::new()
        .dir("/System/Library")
        .dir("/System/Volumes/Data")
        .dir("/Applications")
        .dir("/Library")
        .dir("/usr/local/bin")
        .dir("/opt/homebrew")
        .dir("/private/etc")
        .dir("/private/tmp")
        .dir("/private/var/db/powerlog/Library/PerfPowerTelemetry/S")
        .dir("/Users/Shared")
        .dir("/Users/alice/Library/Caches")
        .dir("/Users/alice/Documents")
        .symlink("/var", "private/var")
        .symlink("/tmp", "private/tmp")
        .symlink("/etc", "private/etc")
        .alias("/System/Volumes/Data/Users", "/Users")
        .alias("/Sýstem", "/System")
        .alias("/Üsers", "/Users")
}

fn guard(fs: &FakeProbe) -> Guard {
    let env = |_: &str| None;
    Guard::new(
        GuardInputs {
            os: Os::MacOs,
            home: p(HOME),
            env: &env,
        },
        fs,
    )
    .unwrap()
}

fn judge(fs: &FakeProbe, path: &str) -> Verdict {
    guard(fs).pass(fs).evaluate(p(path)).verdict().clone()
}

/// A rule with no owners, as a scanner would hold for a plain cache row.
fn any_rule() -> Arc<Rule> {
    Arc::new(Rule {
        id: RuleId("test.any".into()),
        group: GroupId("test".into()),
        owners: Box::new([]),
        sink: Sink::Delete,
        default_selected: true,
    })
}

fn layer(v: &Verdict) -> Option<Layer> {
    v.kept().map(KeptReason::layer)
}

/// `layer/reason` for a kept path, `eligible` otherwise.
fn outcome(verdict: &Verdict) -> String {
    match verdict.kept() {
        Some(reason) => format!("{}/{}", reason.layer().code(), reason.code()),
        None => "eligible".to_owned(),
    }
}

/// Judges every path with one guard and one pass, and reports every mismatch at once.
fn assert_outcomes(fs: &FakeProbe, cases: &[(&str, &str)]) {
    let guard = guard(fs);
    let mut pass = guard.pass(fs);
    let wrong: Vec<String> = cases
        .iter()
        .filter_map(|(path, want)| {
            let path = path.replace('~', HOME);
            let got = outcome(pass.evaluate(p(&path)).verdict());
            (got != *want).then(|| format!("{path}: got {got}, want {want}"))
        })
        .collect();
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn system_is_kept_as_a_critical_system_path() {
    let v = judge(&mac(), "/System");
    assert_eq!(v.to_string(), "Kept · critical system path");
    assert_eq!(layer(&v), Some(Layer::Critical));
}

#[test]
fn critical_roots_are_kept_and_their_children_are_judged_on_their_own() {
    assert_outcomes(
        &mac(),
        &[
            ("/", "critical/system-folder"),
            ("/Library", "critical/system-folder"),
            ("/Library/Application Support", "critical/system-folder"),
            ("/Applications", "critical/system-folder"),
            ("/Volumes", "critical/system-folder"),
            ("/Network", "critical/system-folder"),
            ("/cores", "critical/system-folder"),
            ("/home", "critical/system-folder"),
            ("/net", "critical/system-folder"),
            ("/opt", "critical/system-folder"),
            ("/opt/homebrew", "critical/system-folder"),
            ("/Users", "critical/system-folder"),
            ("/Users/Shared", "critical/system-folder"),
            ("/private", "critical/system-folder"),
            ("/private/tmp", "critical/system-folder"),
            ("/private/var", "critical/system-folder"),
            ("/private/var/tmp", "critical/system-folder"),
            ("/private/var/folders", "critical/system-folder"),
            ("/Applications/Some.app", "eligible"),
            ("/Library/Application Support/Vendor/x", "eligible"),
            ("/Users/Shared/x", "eligible"),
            ("/opt/tool/x", "eligible"),
            ("/usr/local", "critical/critical-system-path"),
            ("/usr/local/bin/tool", "eligible"),
            ("/opt/homebrew/Cellar/foo", "eligible"),
        ],
    );
}

#[test]
fn system_trees_are_kept_with_everything_below_them() {
    let trees = [
        "/bin",
        "/dev",
        "/sbin",
        "/usr",
        "/System",
        "/Library/Apple",
        "/Library/Extensions",
        "/Library/Keychains",
        "/Applications/Finder.app",
        "/Applications/Safari.app",
        "/Users/Guest",
        "/private/etc",
        "/private/var/db",
        "/private/var/audit",
        "/private/var/root",
        "/Library/Updates",
        "/macOS Install Data",
    ];
    let fs = mac();
    for tree in trees {
        assert_outcomes(
            &fs,
            &[
                (tree, "critical/critical-system-path"),
                (&format!("{tree}/a/b"), "critical/critical-system-path"),
            ],
        );
    }
}

#[test]
fn rows_spelled_through_var_tmp_and_etc_hold_on_a_host_without_those_links() {
    assert_outcomes(
        &FakeProbe::new().dir(HOME),
        &[
            ("/tmp", "critical/system-folder"),
            ("/var", "critical/system-folder"),
            ("/var/root", "critical/system-folder"),
            ("/etc", "critical/critical-system-path"),
            ("/etc/hosts", "critical/critical-system-path"),
            ("/var/db", "critical/critical-system-path"),
            ("/var/db/x", "critical/critical-system-path"),
            ("/var/audit/x", "critical/critical-system-path"),
        ],
    );
}

#[test]
fn every_account_root_is_kept_and_the_empty_variable_collapse_is_caught() {
    assert_outcomes(
        &mac(),
        &[
            ("/Users/alice", "critical/home-root"),
            ("/Users/bob", "critical/home-root"),
            ("/Users/alice/x", "eligible"),
            ("/Users/bob/x", "eligible"),
        ],
    );
}

#[test]
fn analyze_structural_folders_are_kept_and_their_contents_are_not() {
    assert_outcomes(
        &mac().file("/Users/alice/Documents/report.txt"),
        &[
            ("~/Library", "critical/home-folder"),
            ("~/Documents", "critical/home-folder"),
            ("~/Desktop", "critical/home-folder"),
            ("~/Downloads", "critical/home-folder"),
            ("~/Pictures", "critical/home-folder"),
            ("~/Movies", "critical/home-folder"),
            ("~/Music", "critical/home-folder"),
            ("~/Documents/report.txt", "eligible"),
            ("~/Desktop/shot.png", "eligible"),
            ("~/Library/Caches", "critical/structural-folder"),
            ("~/Library/Preferences", "critical/structural-folder"),
            ("~/Library/Containers", "critical/structural-folder"),
            ("~/Library/Group Containers", "critical/structural-folder"),
            ("~/Library/Mobile Documents", "critical/structural-folder"),
            ("/Library/Caches", "critical/structural-folder"),
            ("/Library/Caches/x", "eligible"),
            ("/private/var/folders/ab", "critical/structural-folder"),
            ("/private/var/folders/ab/cdef", "critical/structural-folder"),
            (
                "/private/var/folders/ab/cdef/T",
                "critical/structural-folder",
            ),
            ("/private/var/folders/ab/cdef/T/scratch", "eligible"),
        ],
    );
}

#[test]
fn case_aliases_are_kept_by_spelling() {
    assert_outcomes(
        &mac(),
        &[
            ("/SYSTEM", "critical/critical-system-path"),
            ("/SYSTEM/Library", "critical/critical-system-path"),
            ("/OPT/HOMEBREW", "critical/system-folder"),
            ("/USERS/SHARED", "critical/system-folder"),
            ("/users/ALICE", "critical/home-root"),
            ("/Users/alice/library", "critical/home-folder"),
            ("/Users/alice/DOCUMENTS", "critical/home-folder"),
            ("/users/alice/library/caches", "critical/structural-folder"),
        ],
    );
}

#[test]
fn an_alias_that_folding_misses_is_kept_by_file_identity() {
    let fs = mac();
    for (path, anchor) in [
        ("/Sýstem", "/System"),
        ("/Sýstem/Library/x", "/System"),
        ("/Üsers/bob", "/Users"),
    ] {
        let verdict = judge(&fs, path);
        assert!(
            matches!(&verdict, Verdict::Kept(KeptReason::Critical(hit))
                if hit.how == How::SameFileAs && hit.anchor == p(anchor)),
            "{path}: {verdict:?}"
        );
    }
    assert_eq!(
        judge(&fs, "/Sýstem").to_string(),
        "Kept · critical system path (same folder as /System)"
    );
}

#[test]
fn an_alias_never_grants_what_its_own_spelling_lacks() {
    // `/usr/local/x` is allowed by spelling. The same folder under another name is not.
    let fs = mac().alias("/Üsr", "/usr");
    assert_eq!(judge(&fs, "/usr/local/bin/tool"), Verdict::Eligible);
    assert_eq!(
        outcome(&judge(&fs, "/Üsr/local/bin/tool")),
        "critical/critical-system-path"
    );
}

#[test]
fn an_alias_of_a_path_kept_by_name_is_kept_under_its_stored_spelling() {
    // No row is anchored on these paths, so no file identity was recorded for them.
    let claude = "/Users/alice/Library/Application Support/Claude";
    let chat = "/Users/alice/Library/Caches/com.example.chat";
    let idle = "/Users/alice/Library/Caches/com.example.app";
    let fs = mac()
        .dir(claude)
        .dir(chat)
        .dir(idle)
        .alias(
            "/Users/alice/Library/Application Support/Cl\u{e4}ude",
            claude,
        )
        .alias("/Users/alice/Library/Caches/com.example.ch\u{e4}t", chat)
        .alias("/Users/alice/Library/Caches/com.example.\u{e4}pp", idle)
        .running(7, "/Applications/Chat.app/Contents/MacOS/com.example.chat");

    let alias = "/Users/alice/Library/Application Support/Cl\u{e4}ude";
    let verdict = judge(&fs, alias);
    assert!(
        matches!(&verdict, Verdict::Kept(KeptReason::Shape(ShapeReason::AliasOfKept { stored, layer }))
            if *stored == p(claude) && *layer == Layer::Protection),
        "{verdict:?}"
    );
    assert_eq!(
        verdict.to_string(),
        format!("Kept · another spelling of a kept path {claude}")
    );
    assert_outcomes(
        &fs,
        &[
            (alias, "shape/alias-of-kept"),
            (
                "~/Library/Caches/com.example.ch\u{e4}t",
                "shape/alias-of-kept",
            ),
            ("~/Library/Caches/com.example.\u{e4}pp", "eligible"),
        ],
    );
}

#[test]
fn a_scanned_name_outside_ascii_is_judged_under_its_stored_spelling() {
    let claude = "/Users/alice/Library/Application Support/Claude";
    let alias = "/Users/alice/Library/Application Support/Cl\u{e4}ude";
    let fs = mac().dir(claude).alias(alias, claude);
    let guard = guard(&fs);
    let Seen::Found(seen) = fs.node(&p(alias)) else {
        panic!("fixture");
    };
    let candidate = guard.pass(&fs).candidate(p(alias), seen, &any_rule());
    assert_eq!(outcome(candidate.unwrap().verdict()), "shape/alias-of-kept");
}

#[test]
fn a_path_under_a_folder_that_is_not_there_is_kept() {
    // volfs: `/.vol/<device>/<inode>` names a file, and `/.vol/<device>` does not exist.
    let cache = "/Users/alice/Library/Caches/com.example.app";
    let fs = mac()
        .file("/Library/Keychains/System.keychain")
        .dir(cache)
        .alias("/.vol/1/22", "/Library/Keychains")
        .alias("/data/1/33", cache);
    assert_outcomes(
        &fs,
        &[
            ("/.vol/1/22/System.keychain", "shape/alias-of-kept"),
            ("/.vol/1/22", "shape/alias-of-kept"),
            ("/.vol/1/99", "critical/critical-system-path"),
            ("/.nofollow/x", "critical/critical-system-path"),
            ("/.resolve/1/x", "critical/critical-system-path"),
            ("/data/1/33", "shape/parent-not-found"),
            ("/data/1/99", "eligible"),
        ],
    );
    assert_eq!(
        judge(&fs, "/data/1/33").to_string(),
        "Kept · cannot tell what it names: its folder /data/1 is not there"
    );
}

#[test]
fn a_protected_anchor_that_is_a_link_does_not_protect_its_target() {
    // Windows ships `C:\Documents and Settings` as a junction to `C:\Users`. Binding the
    // anchor to the folder behind such a link would keep every file in every profile.
    let fs = mac()
        .file("/Users/Shared/guest-data/file")
        .symlink("/Users/Guest", "/Users/Shared/guest-data");
    assert_outcomes(
        &fs,
        &[
            ("/Users/Shared/guest-data/file", "eligible"),
            ("/Users/Shared/guest-data", "eligible"),
            ("/Users/Guest", "critical/critical-system-path"),
            ("/Users/Guest/file", "shape/symlink-ancestor"),
        ],
    );
}

#[test]
fn a_symlinked_ancestor_is_kept_at_layer_one() {
    let fs = mac().symlink("/Users/alice/link", "/System");
    let v = judge(&fs, "/Users/alice/link/Library");
    assert_eq!(
        v,
        Verdict::Kept(KeptReason::Shape(ShapeReason::LinkedAncestor {
            link: p("/Users/alice/link")
        }))
    );
    assert_eq!(v.to_string(), "Kept · symlinked ancestor /Users/alice/link");
}

#[test]
fn a_redirected_cache_root_refuses_everything_below_it() {
    let fs = mac()
        .file("/Users/alice/Documents/thesis.txt")
        .dir("/Users/alice/real/sub")
        .symlink("/Users/alice/redirect", "/Users/alice/Documents")
        .symlink("/Users/alice/plain", "/Users/alice/real");
    assert_outcomes(
        &fs,
        &[
            ("~/redirect/thesis.txt", "shape/symlink-ancestor"),
            ("~/plain/sub", "shape/symlink-ancestor"),
            ("/var/db/x", "shape/symlink-ancestor"),
            ("/tmp/x", "shape/symlink-ancestor"),
            ("/etc/hosts", "shape/symlink-ancestor"),
        ],
    );
}

#[test]
fn a_leaf_link_is_judged_by_its_target_and_only_ever_refused() {
    let fs = mac()
        .file("/Users/alice/real/file")
        .symlink("/Users/alice/to-system", "/System/Library")
        .symlink("/Users/alice/to-docs", "Documents")
        .symlink("/Users/alice/to-file", "real/file")
        .symlink("/Users/alice/dangling", "/nowhere/at/all")
        .symlink(
            "/Users/alice/Library/Caches/com.apple.Safari",
            "/Users/alice/real",
        );
    let v = judge(&fs, "/Users/alice/to-system");
    assert_eq!(
        v,
        Verdict::Kept(KeptReason::Shape(ShapeReason::LinkToProtected {
            target: p("/System/Library"),
            layer: Layer::Critical,
        }))
    );
    assert_outcomes(
        &fs,
        &[
            ("/var", "shape/link-to-protected"),
            ("/tmp", "shape/link-to-protected"),
            ("/etc", "shape/link-to-protected"),
            ("~/to-docs", "shape/link-to-protected"),
            ("~/to-file", "eligible"),
            ("~/dangling", "eligible"),
            // An eligible target does not make a protected name eligible.
            (
                "~/Library/Caches/com.apple.Safari",
                "protection/protected-bundle",
            ),
        ],
    );
}

#[test]
fn an_unreadable_folder_keeps_everything_below_it() {
    let fs = mac().unreadable("/Users/alice/locked");
    let v = judge(&fs, "/Users/alice/locked/x/y");
    assert_eq!(
        v,
        Verdict::Kept(KeptReason::Shape(ShapeReason::Unreadable {
            at: p("/Users/alice/locked"),
            why: Unseen::Denied
        }))
    );
    assert_eq!(
        v.to_string(),
        "Kept · cannot inspect /Users/alice/locked: permission denied"
    );
    assert_eq!(
        outcome(&judge(&fs, "/Users/alice/locked")),
        "shape/unreadable"
    );
}

#[test]
fn a_mount_point_is_kept_and_its_contents_are_judged_on_their_own() {
    let fs = mac()
        .mount("/Users/alice/mnt/disk", 7)
        .mount("/Volumes/USB", 8);
    assert_outcomes(
        &fs,
        &[
            ("~/mnt/disk", "shape/mount-point"),
            ("/Volumes/USB", "shape/mount-point"),
            ("~/mnt", "eligible"),
        ],
    );
}

#[test]
fn a_data_volume_spelling_is_judged_as_its_root_spelling() {
    let fs = mac().dir("/Users/alice/x");
    assert_outcomes(
        &fs,
        &[
            ("/System/Volumes/Data/Users/alice/x", "eligible"),
            ("/System/Volumes/Data/Users/alice", "critical/home-root"),
            (
                "/System/Volumes/Data/Users/alice/Documents",
                "critical/home-folder",
            ),
            (
                "/System/Volumes/Data/.Spotlight-V100",
                "critical/critical-system-path",
            ),
            // `/Library` exists in this fixture but the Data volume has no entry for it.
            (
                "/System/Volumes/Data/Library/x",
                "critical/critical-system-path",
            ),
        ],
    );
}

#[test]
fn carve_outs_are_eligible_and_their_edges_are_not() {
    assert_outcomes(
        &mac(),
        &[
            (
                "/System/Library/Caches/com.apple.coresymbolicationd/data",
                "eligible",
            ),
            (
                "/System/Library/Caches/com.apple.coresymbolicationd/data/x",
                "eligible",
            ),
            (
                "/System/Library/Caches/com.apple.coresymbolicationd",
                "critical/critical-system-path",
            ),
            ("/private/tmp/x", "eligible"),
            ("/private/var/tmp/x", "eligible"),
            ("/private/var/log", "eligible"),
            ("/private/var/log/x", "eligible"),
            ("/private/var/db/diagnostics", "eligible"),
            ("/private/var/db/DiagnosticPipeline/x", "eligible"),
            ("/private/var/db/powerlog", "eligible"),
            ("/private/var/db/reportmemoryexception/x", "eligible"),
            ("/private/var/db/receipts/a.bom", "eligible"),
            ("/private/var/db/receipts/a.plist", "eligible"),
            (
                "/private/var/db/receipts/a.txt",
                "critical/critical-system-path",
            ),
            ("/private/var/db/receipts", "critical/critical-system-path"),
            // A direct child of the Trash skips the protection layer; a grandchild does not.
            ("~/.Trash/com.apple.foo", "eligible"),
            ("~/.Trash/old/com.apple.foo", "protection/protected-bundle"),
        ],
    );
}

#[test]
fn a_cache_folder_with_an_idle_owner_is_eligible() {
    let fs = mac().dir("/Users/alice/Library/Caches/com.example.app");
    assert_eq!(
        judge(&fs, "/Users/alice/Library/Caches/com.example.app"),
        Verdict::Eligible
    );
}

#[test]
fn a_running_or_unknown_owner_keeps_its_cache() {
    let cache = "/Users/alice/Library/Caches/com.google.Chrome";
    let chrome = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
    let running = mac().dir(cache).running(42, chrome);
    let v = judge(&running, cache);
    assert!(
        matches!(&v, Verdict::Kept(KeptReason::Live(LiveReason::OwnerRunning { owner, .. }))
            if *owner == Owner::BundleId("com.google.Chrome".into())),
        "{v:?}"
    );
    assert_eq!(
        v.to_string(),
        "Kept · com.google.Chrome is running (pid 42); quit it first"
    );
    assert_eq!(
        outcome(&judge(&running, &format!("{cache}/Default/Cache/data_1"))),
        "live/owner-running"
    );

    for unknown in [
        mac().dir(cache).process_table_unknown(),
        mac().dir(cache).process_table_incomplete(),
    ] {
        let v = judge(&unknown, cache);
        assert_eq!(outcome(&v), "live/owner-unknown");
        assert_eq!(
            v.to_string(),
            "Kept · cannot tell whether com.google.Chrome is running: permission denied"
        );
    }
}

#[test]
fn every_user_cache_scope_asks_for_its_owner() {
    let chat = "/Applications/Chat.app/Contents/MacOS/com.example.chat";
    let fs = mac().running(7, chat);
    assert_outcomes(
        &fs,
        &[
            ("~/Library/Caches/com.example.chat", "live/owner-running"),
            (
                "~/Library/Caches/com.example.chat/a/b",
                "live/owner-running",
            ),
            (
                "~/Library/Containers/com.example.chat/Data/Library/Caches/x",
                "live/owner-running",
            ),
            (
                "~/Library/Group Containers/com.example.chat/Caches/x",
                "live/owner-running",
            ),
            (
                "~/Library/Group Containers/com.example.chat/Library/Caches/x",
                "live/owner-running",
            ),
            ("~/library/caches/com.example.chat", "live/owner-running"),
            (
                "~/Library/Containers/com.example.chat/Data/Documents/x",
                "eligible",
            ),
            ("~/Library/Caches/com.example.other/x", "eligible"),
        ],
    );
}

#[test]
fn a_named_cache_tree_has_no_owner_to_ask() {
    let fs = mac().process_table_unknown();
    assert_eq!(
        judge(
            &fs,
            "/Users/alice/Library/Caches/Homebrew/downloads/x.tar.gz"
        ),
        Verdict::Eligible
    );
    assert_eq!(fs.process_calls(), 0);
}

#[test]
fn a_live_powerlog_store_is_kept_inside_the_allowlisted_tree() {
    let s = "/private/var/db/powerlog/Library/PerfPowerTelemetry/S";
    let fs = mac()
        .file(&format!("{s}/CurrentFoo.SQL"))
        .file(&format!("{s}/ArchivedFoo.SQL"));
    assert_outcomes(
        &fs,
        &[
            (&format!("{s}/CurrentFoo.SQL"), "live/live-database"),
            (&format!("{s}/currentfoo.sql"), "live/live-database"),
            (&format!("{s}/CurrentFoo.SQL-wal"), "live/live-database"),
            (&format!("{s}/ArchivedFoo.SQL"), "eligible"),
            (&format!("{s}/deeper/CurrentFoo.SQL"), "eligible"),
        ],
    );
}

#[test]
fn an_open_sqlite_companion_keeps_the_whole_family() {
    let dir = "/Users/alice/Library/Caches/com.example.app";
    let fs = mac()
        .file(&format!("{dir}/x.db"))
        .file(&format!("{dir}/x.db-wal"))
        .file(&format!("{dir}/x.db-shm"))
        .file(&format!("{dir}/y.sqlite3"))
        .open_file(&format!("{dir}/x.db-wal"));
    assert_outcomes(
        &fs,
        &[
            (&format!("{dir}/x.db"), "live/file-open"),
            (&format!("{dir}/x.db-shm"), "live/file-open"),
            (&format!("{dir}/x.db-journal"), "live/file-open"),
            (&format!("{dir}/y.sqlite3"), "eligible"),
            (&format!("{dir}/notes.txt"), "eligible"),
        ],
    );
    let upper = mac()
        .file(&format!("{dir}/X.DB"))
        .open_file(&format!("{dir}/X.DB-WAL"));
    assert_eq!(
        outcome(&judge(&upper, &format!("{dir}/X.DB-WAL"))),
        "live/file-open"
    );
    assert_eq!(
        outcome(&judge(&upper, &format!("{dir}/X.DB"))),
        "live/file-open"
    );
}

#[test]
fn an_unanswered_open_file_probe_keeps_the_family() {
    let fs = mac().in_use_unknown();
    let v = judge(&fs, "/Users/alice/Library/Caches/com.example.app/x.sqlite");
    assert_eq!(
        v,
        Verdict::Kept(KeptReason::Live(LiveReason::InUseUnknown(Unseen::Denied)))
    );
    assert_eq!(
        v.to_string(),
        "Kept · cannot tell whether a process has it open: permission denied"
    );
}

#[test]
fn a_partial_download_is_kept_while_it_is_open() {
    let fs = mac().open_file("/Users/alice/Downloads/movie.mp4.crdownload");
    assert_outcomes(
        &fs,
        &[
            ("~/Downloads/movie.mp4.crdownload", "live/file-open"),
            ("~/Downloads/other.part", "eligible"),
            ("~/Downloads/done.download", "eligible"),
            ("~/Downloads/sub/nested.part", "eligible"),
        ],
    );
    let brew = "/Users/alice/Library/Caches/Homebrew/downloads/abc--pkg.incomplete";
    assert_eq!(
        outcome(&judge(&mac().open_file(brew), brew)),
        "live/file-open"
    );
}

#[test]
fn an_endpoint_security_cache_is_kept_inside_the_allowlisted_tree() {
    assert_outcomes(
        &mac(),
        &[
            (
                "/private/var/folders/ab/cdef/C/com.crowdstrike.falcon.Agent/x",
                "live/edr-cache",
            ),
            (
                "/private/var/folders/ab/cdef/C/COM.SENTINELONE.agent",
                "live/edr-cache",
            ),
            (
                "/private/var/folders/ab/cdef/C/com.cisco.secureclient.gui/x",
                "live/edr-cache",
            ),
            (
                "/private/var/folders/ab/cdef/C/com.example.app/x",
                "eligible",
            ),
        ],
    );
}

#[test]
fn protection_rows_keep_named_state() {
    assert_outcomes(
        &mac(),
        &[
            ("~/.prometheus/data", "protection/user-data"),
            ("~/.prometheus/data/wal/0001", "protection/user-data"),
            ("~/.jupyter/runtime/kernel.json", "protection/user-data"),
            ("~/.yarn/berry/cache/pkg.zip", "protection/user-data"),
            ("~/.gitconfig.lock", "protection/user-data"),
            ("~/.gitconfig.bak.1", "protection/user-data"),
            ("~/.config/fish/fish_history.bak", "protection/user-data"),
            ("~/.bash_history.bak2", "protection/user-data"),
            ("~/.zsh_history.bak", "protection/user-data"),
            ("~/.zsh_history", "eligible"),
            ("~/.cache", "protection/shared-root"),
            ("~/.config", "protection/shared-root"),
            ("~/.local", "protection/shared-root"),
            ("~/.local/bin", "protection/shared-root"),
            ("~/.local/lib", "protection/shared-root"),
            ("~/.local/share", "protection/shared-root"),
            ("~/.local/state", "protection/shared-root"),
            ("~/Library/Unity", "critical/structural-folder"),
            (
                "~/Library/Application Support/Epic",
                "protection/shared-root",
            ),
            ("~/.cache/tool/blob", "eligible"),
            ("~/.CACHE", "protection/shared-root"),
            (
                "~/Library/Containers/com.docker.docker",
                "protection/app-state",
            ),
            (
                "~/Library/Containers/com.docker.docker/Data/vms/0/data",
                "protection/app-state",
            ),
            ("~/.orbstack", "protection/app-state"),
            ("~/.orbstack/run/x", "protection/app-state"),
            (
                "~/Library/Group Containers/HUAQ24HBR6.dev.orbstack",
                "protection/app-state",
            ),
            (
                "~/Library/Group Containers/HUAQ24HBR6.dev.orbstack/data/x",
                "protection/app-state",
            ),
            (
                "~/Library/Caches/SystemSettings.cache/x",
                "protection/app-state",
            ),
            (
                "~/Library/Preferences/com.apple.systempreferences.plist",
                "protection/app-state",
            ),
            (
                "~/Library/Application Support/ControlCenter/x",
                "protection/app-state",
            ),
            (
                "~/Library/Group Containers/group.com.apple.notes/x",
                "protection/user-data",
            ),
            (
                "~/Library/Preferences/com.apple.finder.plist",
                "protection/app-state",
            ),
            (
                "~/Library/Preferences/com.apple.dock.plist",
                "protection/app-state",
            ),
            (
                "~/Library/Application Support/x/com.apple.sharedfilelist/com.apple.Settings.sfl",
                "protection/app-state",
            ),
            (
                "~/Library/Preferences/com.apple.coreaudio.plist",
                "protection/app-state",
            ),
            (
                "~/Library/Preferences/Audio/com.apple.audio.DeviceSettings.plist",
                "protection/app-state",
            ),
            (
                "~/Library/Logs/dustpan/operations.log",
                "protection/user-data",
            ),
            (
                "~/Library/Application Support/Codex/state.json",
                "protection/user-data",
            ),
            (
                "~/Library/Application Support/Codex/Crashpad/pending/a.dmp",
                "eligible",
            ),
            (
                "~/Library/Application Support/Codex/Crashpad/pending/a/b.dmp",
                "protection/user-data",
            ),
            (
                "~/Library/Application Support/Codex/Crashpad/completed/a.dmp",
                "protection/user-data",
            ),
            (
                "~/Library/Logs/com.openai.codex/x.log",
                "protection/user-data",
            ),
            ("~/.codex/sessions/2026/x.jsonl", "protection/user-data"),
            ("~/.codex/auth.json", "protection/user-data"),
            ("~/.codex/history.jsonl", "protection/user-data"),
            ("~/.codex/session_index.jsonl", "protection/user-data"),
            ("~/.codex/cache/session_index.jsonl", "protection/user-data"),
            (
                "~/.codex/cache/codex_app_directory/x",
                "protection/user-data",
            ),
            ("~/.codex/tmp/x", "eligible"),
            ("~/.claude", "protection/user-data"),
            ("~/.claude/projects/p/1.jsonl", "protection/user-data"),
            ("~/.claude/file-history/x", "protection/user-data"),
            ("~/.claude/history.jsonl", "protection/user-data"),
            ("~/.CLAUDE/PROJECTS/p", "protection/user-data"),
            ("~/.claude/worktrees/w", "eligible"),
            (
                "~/Library/Preferences/ByHost/com.apple.bluetooth.ABC.plist",
                "protection/user-data",
            ),
            (
                "~/Library/Preferences/ByHost/com.apple.wifi.ABC.plist",
                "protection/user-data",
            ),
            (
                "~/Library/Preferences/com.apple.networkextension.plist",
                "protection/user-data",
            ),
            (
                "~/Library/Mobile Documents/com~apple~CloudDocs",
                "protection/cloud-structure",
            ),
            (
                "~/Library/Mobile Documents/com~apple~CloudDocs/Documents/a.txt",
                "protection/cloud-structure",
            ),
            (
                "/Volumes/Ext/Mobile Documents.bak/x",
                "protection/cloud-structure",
            ),
            ("~/Library/Accounts/Accounts4.plist", "protection/user-data"),
            (
                "~/Library/Keychains/login.keychain-db",
                "protection/user-data",
            ),
            ("~/Library/Mail/V10/x", "protection/user-data"),
            ("~/Library/Calendars/x", "protection/user-data"),
            ("~/Library/Contacts/x", "protection/user-data"),
            (
                "/Library/Audio/Plug-Ins/Components/x.component",
                "protection/user-data",
            ),
            ("/Library/Audio/Plug-Ins/VST/x.vst", "protection/user-data"),
            (
                "/Library/Audio/Plug-Ins/VST3/x.vst3",
                "protection/user-data",
            ),
            (
                "/Library/Application Support/iZotope/x",
                "protection/user-data",
            ),
            (
                "~/Library/Application Support/iZotope/x",
                "protection/user-data",
            ),
            (
                "/Library/Application Support/LaserSoft Imaging/x",
                "protection/user-data",
            ),
            (
                "~/Library/Preferences/com.native-instruments.Kontakt.plist",
                "protection/user-data",
            ),
            (
                "~/Library/Preferences/com.avid.mediacomposer.2024.plist",
                "protection/user-data",
            ),
            (
                "~/Library/Preferences/com.fabfilter.Pro-Q.3.plist",
                "protection/user-data",
            ),
            (
                "~/Library/Preferences/com.paceap.eden.plist",
                "protection/user-data",
            ),
            (
                "/private/var/folders/ab/cdef/C/com.native-instruments.x/y",
                "protection/user-data",
            ),
            (
                "/private/var/folders/ab/cdef/C/com.avid.mediacomposer",
                "protection/user-data",
            ),
            (
                "/private/var/folders/ab/cdef/C/com.paceap.eden.iLokLicenseManager",
                "protection/user-data",
            ),
            (
                "~/Library/Caches/ms-playwright/chromium-1/x",
                "protection/user-data",
            ),
            ("~/Library/Caches/camoufox", "protection/user-data"),
            (
                "~/Library/Caches/Cypress/13.0.0/Cypress.app",
                "protection/user-data",
            ),
            ("~/.cache/puppeteer/chrome/x", "protection/user-data"),
            (
                "~/Library/Caches/app.cotypist.Cotypist/model",
                "protection/user-data",
            ),
            (
                "~/Library/Caches/com.displaylink.DisplayLinkUserAgent/x",
                "protection/app-state",
            ),
            (
                "~/Library/Caches/com.lasersoft-imaging.SilverFast9/x",
                "protection/user-data",
            ),
            (
                "~/Library/Caches/Adobe Camera Raw 2/x",
                "protection/user-data",
            ),
            (
                "~/Library/Caches/com.apple.containermanagerd/x",
                "protection/app-state",
            ),
            ("~/Library/Caches/com.apple.homed/x", "protection/app-state"),
            (
                "~/Library/Caches/com.apple.ap.adprivacyd/x",
                "protection/app-state",
            ),
            ("~/Library/Caches/FamilyCircle/x", "protection/app-state"),
            (
                "~/Library/Caches/com.apple.HomeKit/x",
                "protection/app-state",
            ),
            (
                "~/Library/Caches/com.apple.siriactionsd.ShortcutsSandboxCache/x",
                "protection/app-state",
            ),
            (
                "~/Library/Caches/GIPPseudonymousID/x",
                "protection/app-state",
            ),
            (
                "~/Library/Caches/CCTClearcutLogger/x",
                "protection/app-state",
            ),
            (
                "~/Library/Application Support/com.apple.idleassetsd/Customer/x",
                "protection/user-data",
            ),
            (
                "~/Library/Application Support/com.apple.wallpaper/x",
                "protection/user-data",
            ),
        ],
    );
}

#[test]
fn bundle_ids_keep_app_data_by_name() {
    assert_outcomes(
        &mac(),
        &[
            (
                "~/Library/Caches/com.apple.Safari",
                "protection/protected-bundle",
            ),
            ("~/Library/Caches/com.apple.Safari/Cache.data", "eligible"),
            (
                "~/Library/Preferences/org.cups.PrintingPrefs.plist",
                "protection/protected-bundle",
            ),
            (
                "~/Library/Application Support/Claude",
                "protection/protected-bundle",
            ),
            ("~/Library/Application Support/Claude/logs", "eligible"),
            (
                "~/Library/Caches/com.microsoft.VSCode",
                "protection/protected-bundle",
            ),
            (
                "~/Library/Caches/com.tencent.xinWeChat",
                "protection/protected-bundle",
            ),
            ("~/Library/Caches/com.example.app", "eligible"),
            (
                "~/Library/Application Support/Tailscale-tailscaled/state/x",
                "protection/protected-bundle",
            ),
            (
                "~/Library/Application Support/cloudflare/x/warp/y",
                "protection/protected-bundle",
            ),
            (
                "~/Library/Containers/com.apple.Safari/Data/Documents/x",
                "protection/protected-bundle",
            ),
            (
                "~/Library/Containers/com.apple.Safari/Data/Library/Preferences/x.plist",
                "protection/protected-bundle",
            ),
            (
                "~/Library/Containers/com.apple.Safari/Data/Library/Caches/blob",
                "eligible",
            ),
            (
                "~/Library/Containers/com.apple.Safari/Data/Library/Caches/com.apple.Safari",
                "eligible",
            ),
            (
                "~/Library/Containers/com.apple.Safari/Data/tmp/x",
                "eligible",
            ),
            (
                "~/Library/Containers/com.apple.Safari/Data/Library/Caches",
                "protection/protected-bundle",
            ),
            (
                "~/Library/Group Containers/group.com.apple.shortcuts/x",
                "eligible",
            ),
            (
                "~/Library/Containers/com.example.app/Data/Documents/x",
                "eligible",
            ),
            (
                "~/Library/Containers/com.tencent.xinWeChat/Data/Documents/app_data/log/x",
                "eligible",
            ),
            (
                "~/Library/Containers/com.tencent.xinWeChat/Data/Documents/app_data/db/x",
                "protection/protected-bundle",
            ),
            (
                "~/Library/Containers/com.apple.Safari/Data/Library/Caches/com.apple.finder/x",
                "protection/app-state",
            ),
            ("~/Library/Caches/Codex", "protection/protected-bundle"),
            ("~/Library/Caches/Codex/Default/Cache/data_0", "eligible"),
        ],
    );
}

#[test]
fn layers_decide_in_order() {
    let chrome = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
    let fs = mac()
        .running(1, chrome)
        .symlink("/Users/alice/Library/Caches/link", "/System");
    assert_outcomes(
        &fs,
        &[
            ("~/Library/Caches/link/x", "shape/symlink-ancestor"),
            ("~/Library/Caches/com.google.Chrome", "live/owner-running"),
            ("/private/var/db/x.db", "critical/critical-system-path"),
        ],
    );
}

#[test]
fn an_unreadable_protected_anchor_means_no_guard() {
    let fs = mac().unreadable("/System");
    let env = |_: &str| None;
    let built = Guard::new(
        GuardInputs {
            os: Os::MacOs,
            home: p(HOME),
            env: &env,
        },
        &fs,
    );
    assert!(matches!(
        built,
        Err(PolicyError::AnchorUnreadable {
            why: Unseen::Denied,
            ..
        })
    ));
}

#[test]
fn a_home_that_is_a_system_root_means_no_guard() {
    let env = |_: &str| None;
    for home in ["/", "/Users", "/usr/local", "/System/x", "/private/var"] {
        let built = Guard::new(
            GuardInputs {
                os: Os::MacOs,
                home: p(home),
                env: &env,
            },
            &mac(),
        );
        assert!(
            matches!(&built, Err(PolicyError::HomeIsCritical(h)) if *h == p(home)),
            "{home}"
        );
    }
}

#[test]
fn every_reason_has_a_stable_code_a_layer_and_a_sentence() {
    let chrome = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
    let fs = mac()
        .running(1, chrome)
        .unreadable("/Users/alice/locked")
        .mount("/Volumes/USB", 9)
        .symlink("/Users/alice/link", "/System")
        .dir("/Users/alice/Library/Application Support/Claude")
        .alias(
            "/Users/alice/Library/Application Support/Cl\u{e4}ude",
            "/Users/alice/Library/Application Support/Claude",
        )
        .dir("/Users/alice/Library/Caches/com.example.idle")
        .alias("/data/1/2", "/Users/alice/Library/Caches/com.example.idle")
        .open_file("/Users/alice/x.db");
    let guard = guard(&fs);
    let mut pass = guard.pass(&fs);
    let mut codes = Vec::new();
    for path in [
        "/System",
        "/",
        "/Users/bob",
        "/Users/alice/Documents",
        "/Users/alice/Library/Caches",
        "/Users/alice/.cache",
        "/Users/alice/Library/Mobile Documents/x/y",
        "/Users/alice/Library/Application Support/Claude",
        "/Users/alice/.orbstack",
        "/Users/alice/.claude",
        "/Users/alice/link/x",
        "/Users/alice/link",
        "/Users/alice/locked/x",
        "/Users/alice/Library/Application Support/Cl\u{e4}ude",
        "/data/1/2",
        "/Volumes/USB",
        "/Users/alice/Library/Caches/com.google.Chrome",
        "/Users/alice/x.db",
        "/private/var/db/powerlog/Library/PerfPowerTelemetry/S/CurrentX.SQL",
        "/private/var/folders/a/b/C/com.eset.x",
    ] {
        let verdict = pass.evaluate(p(path)).verdict().clone();
        let reason = verdict
            .kept()
            .unwrap_or_else(|| panic!("{path} is eligible"));
        let sentence = reason.to_string();
        assert!(
            !sentence.is_empty() && sentence != reason.code(),
            "{path}: {sentence}"
        );
        assert_eq!(reason.source(), None, "{path}");
        codes.push(reason.code());
    }
    assert_eq!(
        codes,
        [
            "critical-system-path",
            "system-folder",
            "home-root",
            "home-folder",
            "structural-folder",
            "shared-root",
            "cloud-structure",
            "protected-bundle",
            "app-state",
            "user-data",
            "symlink-ancestor",
            "link-to-protected",
            "unreadable",
            "alias-of-kept",
            "parent-not-found",
            "mount-point",
            "owner-running",
            "file-open",
            "live-database",
            "edr-cache",
        ]
    );
}

#[test]
fn siblings_share_their_ancestors_lstats() {
    let mut fs = mac();
    for i in 0..1000 {
        fs = fs.file(&format!("/Users/alice/Library/Caches/com.example.app/f{i}"));
    }
    let guard = guard(&fs);
    let before = fs.node_calls();
    let mut pass = guard.pass(&fs);
    for i in 0..1000 {
        pass.evaluate(p(&format!(
            "/Users/alice/Library/Caches/com.example.app/f{i}"
        )));
    }
    // `/`, Users, alice, Library, Caches, com.example.app once each, then one lstat per file.
    assert_eq!(fs.node_calls() - before, 6 + 1000);
    assert_eq!(fs.process_calls(), 1);
}

#[test]
fn kinds_are_reported_with_the_row_that_matched() {
    let fs = mac().file("/Users/alice/Documents/report.txt");
    assert!(
        matches!(judge(&fs, "/Users/alice/Documents"), Verdict::Kept(KeptReason::Critical(ref h))
            if h.kind == Kind::HomeFolder && h.how == How::Exact)
    );
    let inside = judge(&fs, "/System/Library/x");
    assert!(
        matches!(&inside, Verdict::Kept(KeptReason::Critical(h))
            if h.how == How::Inside && h.anchor == p("/System")),
        "{inside:?}"
    );
    assert_eq!(
        inside.to_string(),
        "Kept · critical system path (inside /System)"
    );
    let child = judge(&fs, "/Users/bob");
    assert!(
        matches!(&child, Verdict::Kept(KeptReason::Critical(h))
            if h.how == How::ChildOf && h.anchor == p("/Users")),
        "{child:?}"
    );
}
