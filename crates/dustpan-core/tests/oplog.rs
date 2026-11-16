//! The operations log against an in-memory store: escaping, rotation, a log that cannot be
//! written, and reading back what dustpan and Mole wrote. The read-only log folder on a real
//! disk is live lane 8.
#![cfg(unix)]

use dustpan_core::candidate::{GroupId, Rule, RuleId, Sink};
use dustpan_core::guard::{Guard, GuardInputs, Homes, Lease};
use dustpan_core::history::{Attribution, History, Limit};
use dustpan_core::oplog::{Command, LogStep, MemoryStore, OpLog, ROTATE_AT, RunId};
use dustpan_core::path::AbsPath;
use dustpan_core::paths::Os;
use dustpan_core::probe::fake::FakeProbe;
use dustpan_core::probe::{Probe, Seen};
use dustpan_core::sink::{Cancel, Executor, Mode, Outcome, Services};
use dustpan_core::time::{Clock, FixedClock, WallTime};
use dustpan_core::whitelist::Whitelist;
use std::io::ErrorKind;
use std::sync::Arc;

const CACHES: &str = "/Users/alice/Library/Caches";

fn p(s: &str) -> AbsPath {
    AbsPath::parse(s.as_ref()).unwrap()
}

/// Runs the Delete sink over `names` in `CACHES` with `store` as the log, and returns the
/// outcomes, the warnings and the store.
fn clean(
    fs: &FakeProbe,
    names: &[&str],
    store: MemoryStore,
) -> (
    Vec<Outcome>,
    Vec<dustpan_core::oplog::LogWarning>,
    MemoryStore,
) {
    let env = |_: &str| None;
    let home = p("/Users/alice");
    let inputs = GuardInputs {
        os: Os::MacOs,
        homes: Homes::new(home.clone(), None),
        env: &env,
        whitelist: Whitelist::defaults(Os::MacOs, &home),
    };
    let guard = Guard::new(inputs, fs).unwrap();
    let rule = Arc::new(Rule {
        id: RuleId("test.cache".into()),
        group: GroupId("test".into()),
        owners: Box::new([]),
        sink: Sink::Delete,
        default_selected: true,
    });
    let mut pass = guard.pass(fs);
    let plan: Vec<_> = names
        .iter()
        .map(|name| {
            let path = fs.listed(CACHES, name);
            let Seen::Found(node) = fs.node(&path) else {
                panic!("no {name}")
            };
            pass.candidate(path, node, &rule).unwrap()
        })
        .collect();
    let clock = FixedClock::new(WallTime {
        unix_secs: 1_763_280_000,
        utc_offset_secs: 7 * 3_600,
    });
    let cancel = Cancel::default();
    let mut warnings = Vec::new();
    let mut log = OpLog::new(store, |w: &_| warnings.push(Clone::clone(w)));
    let services = Services {
        guard: &guard,
        probe: fs,
        clock: &clock,
        lease: Lease::BATCH,
        journal: &mut log,
        cancel: &cancel,
    };
    let run = RunId::new(clock.wall(), 4242, 0x9f3a);
    let mut executor = Executor::start(Mode::Apply(fs), Command::CLEAN, run, services);
    let outcomes = plan.into_iter().map(|c| executor.run(c).1).collect();
    executor.finish();
    let store = log.into_store();
    (outcomes, warnings, store)
}

#[test]
fn a_newline_in_a_name_is_one_row_with_a_literal_backslash_n() {
    let fs = FakeProbe::new().file_in(CACHES, "bad\nname");
    let (outcomes, _, store) = clean(&fs, &["bad\nname"], MemoryStore::default());
    assert!(matches!(outcomes[0], Outcome::Removed(_)), "{outcomes:?}");
    let text = String::from_utf8(store.current.clone()).unwrap();
    let rows: Vec<&str> = text.lines().filter(|l| l.starts_with('[')).collect();
    assert_eq!(rows.len(), 1, "{text}");
    assert!(
        rows[0].contains(r"REMOVED /Users/alice/Library/Caches/bad\nname (sink=delete"),
        "{text}"
    );
    let history = History::read(b"", &store.current);
    assert_eq!(history.deletions(Limit::DEFAULT).count(), 1);
}

#[test]
fn every_control_byte_is_escaped_in_a_row() {
    let name = "a\rb\tc\x1bd\x7fe";
    let fs = FakeProbe::new().file_in(CACHES, name);
    let (_, _, store) = clean(&fs, &[name], MemoryStore::default());
    let text = String::from_utf8(store.current).unwrap();
    assert!(text.contains(r"a\rb\tc\x1Bd\x7Fe (sink=delete"), "{text}");
    assert!(!text.contains('\r') && !text.contains('\x1b') && !text.contains('\x7f'));
}

#[test]
fn the_log_rotates_once_at_session_start_above_five_megabytes() {
    let full = vec![b'x'; usize::try_from(ROTATE_AT).unwrap() + 1];
    let fs = FakeProbe::new()
        .dir(&format!("{CACHES}/a"))
        .dir(&format!("{CACHES}/b"));
    let store = MemoryStore {
        current: full.clone(),
        ..MemoryStore::default()
    };
    let (_, warnings, store) = clean(&fs, &["a", "b"], store);
    assert!(warnings.is_empty());
    assert_eq!(store.rotations, 1);
    assert_eq!(store.old, full);
    let text = String::from_utf8(store.current).unwrap();
    assert!(text.starts_with("\n# ========== clean run="), "{text}");
    assert_eq!(text.matches("] REMOVED ").count(), 2);
}

#[test]
fn the_log_does_not_rotate_at_five_megabytes_or_less() {
    let fs = FakeProbe::new().dir(&format!("{CACHES}/a"));
    let store = MemoryStore {
        current: vec![b'x'; usize::try_from(ROTATE_AT).unwrap()],
        ..MemoryStore::default()
    };
    let (_, _, store) = clean(&fs, &["a"], store);
    assert_eq!(store.rotations, 0);
}

#[test]
fn an_unwritable_log_folder_warns_once_and_never_stops_the_run() {
    let fs = FakeProbe::new()
        .dir(&format!("{CACHES}/a"))
        .dir(&format!("{CACHES}/b"));
    let store = MemoryStore {
        failing: Some(ErrorKind::PermissionDenied),
        ..MemoryStore::default()
    };
    let (outcomes, warnings, _) = clean(&fs, &["a", "b"], store);
    assert!(
        outcomes.iter().all(|o| matches!(o, Outcome::Removed(_))),
        "{outcomes:?}"
    );
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert_eq!(warnings[0].step, LogStep::Rotate);
    assert_eq!(warnings[0].error, ErrorKind::PermissionDenied);
    assert!(!fs.exists(&format!("{CACHES}/a")) && !fs.exists(&format!("{CACHES}/b")));
}

#[test]
fn a_crashed_session_reads_back_as_not_ended_and_the_next_one_is_whole() {
    let log =
        b"\n# ========== clean run=1-1-1 session started at 2026-11-16 09:00:00 +0700 ==========\n\
[2026-11-16 09:00:01] [clean run=1-1-1] REMOVED /Users/a/Library/Caches/x (sink=delete size=4096)\n\
[2026-11-16 09:00:02] [clean run=1-1-1] REMOV\
\n# ========== clean run=2-1-1 session started at 2026-11-16 10:00:00 +0700 ==========\n\
# ========== clean run=2-1-1 session ended at 2026-11-16 10:00:01, 0 items, 0 B ==========\n";
    let history = History::read(b"", log);
    let sessions: Vec<_> = history.sessions(Limit::DEFAULT).collect();
    assert_eq!(sessions.len(), 2);
    assert_eq!(sessions[1].ended_at, "", "the crashed one");
    assert_eq!(sessions[1].actions.removed, 1);
    assert_eq!(sessions[0].ended_at, "2026-11-16 10:00:01");
}

#[test]
fn rows_rotated_away_from_their_start_still_form_a_session() {
    let old =
        b"[2026-11-16 09:00:01] [clean run=1-1-1] SKIPPED /x (sink=trash reason=owner-running)\n";
    let current = b"# ========== clean run=1-1-1 session ended at 2026-11-16 09:00:02, 0 items, 0 B ==========\n";
    let history = History::read(old, current);
    let sessions: Vec<_> = history.sessions(Limit::DEFAULT).collect();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].actions.skipped, 1);
    assert_eq!(sessions[0].ended_at, "2026-11-16 09:00:02");
    let record = history.deletions(Limit::DEFAULT).next().unwrap();
    assert_eq!(
        (
            record.timestamp.as_str(),
            record.mode,
            record.status.as_str()
        ),
        ("2026-11-16T09:00:01", "trash", "owner-running")
    );
}

#[test]
fn a_mole_log_reads_back_with_its_sessions_and_counts() {
    let log = b"\n# ========== clean run=20261116090000-77-a-b session started at 2026-11-16 09:00:00 ==========\n\
[2026-11-16 09:00:01] [clean run=20261116090000-77-a-b] REMOVED /Users/a/Library/Caches/x (15.2MB)\n\
[2026-11-16 09:00:01] [clean run=20261116090000-77-a-b] SKIPPED /Users/a/Library/Caches/y (whitelist)\n\
[2026-11-16 09:00:02] [optimize] REBUILT /x (spotlight)\n\
# ========== clean run=20261116090000-77-a-b session ended at 2026-11-16 09:00:03, 1 items, 15.2MB ==========\n";
    let history = History::read(b"", log);
    let sessions: Vec<_> = history.sessions(Limit::DEFAULT).collect();
    assert_eq!(sessions.len(), 2);
    let clean = sessions.iter().find(|s| s.command == "clean").unwrap();
    assert_eq!(clean.attribution, Attribution::Run);
    assert_eq!(clean.started_at, "2026-11-16 09:00:00");
    assert_eq!((clean.actions.removed, clean.actions.skipped), (1, 1));
    assert_eq!((clean.items, clean.size.as_str()), (1, "15.2MB"));
    let optimize = sessions.iter().find(|s| s.command == "optimize").unwrap();
    assert_eq!(optimize.attribution, Attribution::Command);
    assert_eq!(optimize.actions.rebuilt, 1);
    assert_eq!(
        history.deletions(Limit::DEFAULT).count(),
        0,
        "Mole keeps those in deletions.log"
    );
}

#[test]
fn invalid_utf8_reads_as_the_replacement_character() {
    let mut log = b"[2026-11-16 09:00:01] [clean run=1-1-1] REMOVED /Users/a/".to_vec();
    log.extend_from_slice(&[0xff, 0xfe]);
    log.extend_from_slice(b" (sink=delete size=1)\n");
    let history = History::read(b"", &log);
    let record = history.deletions(Limit::DEFAULT).next().unwrap();
    assert_eq!(record.path, "/Users/a/\u{fffd}\u{fffd}");
}

#[test]
fn the_newest_come_first_and_the_limit_caps_both_lists() {
    let mut log = Vec::new();
    for i in 0..30 {
        log.extend_from_slice(
            format!("[2026-11-16 09:00:{i:02}] [clean run={i}-1-1] REMOVED /x/{i} (sink=delete size=1)\n")
                .as_bytes(),
        );
    }
    let history = History::read(b"", &log);
    let limit = Limit::new(5).unwrap();
    let sessions: Vec<_> = history.sessions(limit).map(|s| s.run_id.clone()).collect();
    assert_eq!(sessions, ["29-1-1", "28-1-1", "27-1-1", "26-1-1", "25-1-1"]);
    assert_eq!(history.deletions(limit).count(), 5);
    assert_eq!(
        history.deletions(Limit::DEFAULT).next().unwrap().path,
        "/x/29"
    );
}

#[test]
fn a_limit_outside_one_to_two_hundred_is_refused() {
    assert_eq!(Limit::new(0), None);
    assert_eq!(Limit::new(201), None);
    assert_eq!(Limit::new(200).map(Limit::get), Some(200));
    assert_eq!(Limit::new(1).map(Limit::get), Some(1));
}
