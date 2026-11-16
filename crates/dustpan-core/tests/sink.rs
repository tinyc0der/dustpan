//! The executor and both sinks against the fake, which serves the guard and the sinks from one
//! tree, so a race between approval and removal is a test, not a hope. The live versions (a
//! real mount, a real Trash) are `crates/dustpan-sys/tests/sink_fixture.rs`.
#![cfg(unix)]

use dustpan_core::candidate::{Candidate, GroupId, Rule, RuleId, Sink};
use dustpan_core::guard::{
    ContentsReason, Guard, GuardInputs, Homes, IdentityReason, KeptReason, Lease, ShapeReason,
};
use dustpan_core::history::{History, Limit};
use dustpan_core::oplog::{Command, MemoryStore, OpLog, RunId};
use dustpan_core::path::AbsPath;
use dustpan_core::paths::Os;
use dustpan_core::probe::fake::FakeProbe;
use dustpan_core::probe::{Probe, Seen};
use dustpan_core::sink::{Cancel, Executor, FailCause, Mode, Moved, Outcome, Services, Tally};
use dustpan_core::size::Size;
use dustpan_core::time::{Clock, FixedClock, WallTime};
use dustpan_core::whitelist::Whitelist;
use std::path::PathBuf;
use std::sync::Arc;

const CACHES: &str = "/Users/alice/Library/Caches";
const CACHE: &str = "/Users/alice/Library/Caches/com.example.app";
const BLOB: &str = "/Users/alice/Library/Caches/com.example.app/blob";
const INNER: &str = "/Users/alice/Library/Caches/com.example.app/data";

fn p(s: &str) -> AbsPath {
    AbsPath::parse(s.as_ref()).unwrap()
}

fn path(s: &str) -> PathBuf {
    PathBuf::from(s)
}

fn clock() -> FixedClock {
    FixedClock::new(WallTime {
        unix_secs: 1_763_280_000,
        utc_offset_secs: 0,
    })
}

fn rule(sink: Sink) -> Arc<Rule> {
    Arc::new(Rule {
        id: RuleId("test.cache".into()),
        group: GroupId("test".into()),
        owners: Box::new([]),
        sink,
        default_selected: true,
    })
}

fn guard(fs: &FakeProbe) -> Guard {
    let env = |_: &str| None;
    let home = p("/Users/alice");
    let inputs = GuardInputs {
        os: Os::MacOs,
        homes: Homes::new(home.clone(), None),
        env: &env,
        whitelist: Whitelist::defaults(Os::MacOs, &Homes::new(home.clone(), None)),
    };
    Guard::new(inputs, fs).unwrap()
}

fn discover(guard: &Guard, fs: &FakeProbe, at: &str, sink: Sink) -> Candidate {
    let Seen::Found(node) = fs.node(&p(at)) else {
        panic!("fixture has no {at}")
    };
    let mut candidate = guard.pass(fs).candidate(p(at), node, &rule(sink)).unwrap();
    candidate.size = Size::Known(8192);
    candidate
}

struct Run {
    outcome: Outcome,
    tally: Tally,
    log: MemoryStore,
}

/// One candidate through a fresh executor, with its log.
fn run_at(fs: &FakeProbe, at: &str, sink: Sink, apply: bool) -> Run {
    let guard = guard(fs);
    let clock = clock();
    let cancel = Cancel::default();
    let mut log = OpLog::new(MemoryStore::default(), |_: &_| {});
    let candidate = discover(&guard, fs, at, sink);
    let services = Services {
        guard: &guard,
        probe: fs,
        clock: &clock,
        lease: Lease::BATCH,
        journal: &mut log,
        cancel: &cancel,
    };
    let mode = if apply {
        Mode::Apply(fs)
    } else {
        Mode::Preview
    };
    let run = RunId::new(clock.wall(), 1, 1);
    let mut executor = Executor::start(mode, Command::named("test"), run, services);
    let (_, outcome) = executor.run(candidate);
    let tally = executor.finish();
    Run {
        outcome,
        tally,
        log: log.into_store(),
    }
}

fn run(fs: &FakeProbe, sink: Sink, apply: bool) -> Run {
    run_at(fs, CACHE, sink, apply)
}

fn fixture() -> FakeProbe {
    FakeProbe::new()
        .dir("/Users/alice/Documents")
        .file(BLOB)
        .file(&format!("{INNER}/a"))
        .file(&format!("{INNER}/deeper/b"))
}

fn changed() -> Outcome {
    Outcome::Kept(KeptReason::Identity(IdentityReason::Changed))
}

#[test]
fn an_approved_tree_is_removed_deepest_first_and_its_bytes_counted() {
    let fs = fixture();
    let Run {
        outcome,
        tally,
        log,
    } = run(&fs, Sink::Delete, true);
    assert_eq!(outcome, Outcome::Removed(Size::Known(3 * 4096)));
    assert_eq!((tally.removed, tally.freed), (1, Size::Known(3 * 4096)));
    assert!(!fs.exists(CACHE));
    let removed = fs.removed();
    let at = |s: &str| removed.iter().position(|r| *r == path(s)).unwrap();
    assert!(at(&format!("{INNER}/deeper/b")) < at(&format!("{INNER}/deeper")));
    assert!(at(&format!("{INNER}/deeper")) < at(INNER));
    assert_eq!(removed.last(), Some(&path(CACHE)));
    let rows = String::from_utf8_lossy(&log.current);
    assert_eq!(rows.matches("] REMOVED ").count(), 1, "{rows}");
    assert!(
        rows.contains(
            "REMOVED /Users/alice/Library/Caches/com.example.app (sink=delete size=12288)"
        )
    );
}

#[test]
fn an_inode_swapped_after_approval_is_kept_and_nothing_is_removed() {
    let fs = fixture();
    fs.before_sink(|fs| fs.swap(CACHE));
    assert_eq!(run(&fs, Sink::Delete, true).outcome, changed());
    assert!(fs.removed().is_empty());
}

#[test]
fn a_file_swapped_after_approval_is_kept() {
    let fs = fixture();
    fs.before_sink(|fs| fs.swap(BLOB));
    assert_eq!(run_at(&fs, BLOB, Sink::Delete, true).outcome, changed());
    assert!(fs.removed().is_empty());
}

#[test]
fn a_parent_replaced_by_a_symlink_after_approval_is_kept() {
    let fs = fixture().dir("/Users/alice/Documents/com.example.app");
    fs.before_sink(|fs| fs.replace_with_symlink(CACHES, "/Users/alice/Documents"));
    assert_eq!(run(&fs, Sink::Delete, true).outcome, changed());
    assert!(fs.removed().is_empty());
    assert!(fs.exists("/Users/alice/Documents/com.example.app"));
}

#[test]
fn a_mount_point_inside_the_target_keeps_the_whole_target_in_both_modes() {
    for apply in [false, true] {
        let mnt = format!("{INNER}/mnt");
        let fs = fixture().mount(&mnt, 2).file(&format!("{mnt}/kept"));
        let outcome = run(&fs, Sink::Delete, apply).outcome;
        assert!(
            matches!(&outcome, Outcome::Kept(KeptReason::Contents(ContentsReason::Holds { at, reason }))
                if *at == p(&mnt) && **reason == KeptReason::Shape(ShapeReason::MountPoint)),
            "apply={apply}: {outcome:?}"
        );
        assert!(fs.removed().is_empty());
    }
}

#[test]
fn a_mount_that_appears_after_approval_is_not_entered() {
    // A mount over `INNER/deeper` has a `FileId` the survey never judged, so the walk leaves it
    // and everything in it, and says where it stopped.
    let deeper = format!("{INNER}/deeper");
    let fs = fixture();
    let mounted = deeper.clone();
    fs.before_sink(move |fs| fs.swap(&mounted));
    let outcome = run(&fs, Sink::Delete, true).outcome;
    assert!(
        matches!(&outcome, Outcome::Failed(f) if f.cause == FailCause::Changed && f.at == p(&deeper)),
        "{outcome:?}"
    );
    assert!(fs.exists(&deeper));
    assert!(fs.exists(CACHE), "the folder that holds it stays");
}

#[test]
fn a_missing_path_is_success_counted_as_gone() {
    for at in [CACHE, BLOB] {
        let fs = fixture();
        fs.before_sink(move |fs| fs.remove(at));
        let Run { outcome, tally, .. } = run_at(&fs, at, Sink::Delete, true);
        assert_eq!(outcome, Outcome::Gone, "{at}");
        assert_eq!((tally.gone, tally.failed), (1, 0));
    }
}

#[test]
fn a_path_gone_before_the_recheck_is_also_success() {
    let fs = fixture();
    let guard = guard(&fs);
    let candidate = discover(&guard, &fs, CACHE, Sink::Delete);
    fs.remove(CACHE);
    let clock = clock();
    let cancel = Cancel::default();
    let mut log = OpLog::new(MemoryStore::default(), |_: &_| {});
    let services = Services {
        guard: &guard,
        probe: &fs,
        clock: &clock,
        lease: Lease::BATCH,
        journal: &mut log,
        cancel: &cancel,
    };
    let run = RunId::new(clock.wall(), 1, 1);
    let mut executor = Executor::start(Mode::Apply(&fs), Command::named("test"), run, services);
    assert_eq!(executor.run(candidate).1, Outcome::Gone);
}

#[test]
fn a_hard_link_outside_the_target_keeps_its_bytes_out_of_the_freed_total() {
    let fs = fixture().hard_link("/Users/alice/Documents/kept", BLOB);
    let outcome = run(&fs, Sink::Delete, true).outcome;
    assert_eq!(outcome, Outcome::Removed(Size::Known(2 * 4096)));
    assert!(fs.exists("/Users/alice/Documents/kept"));
}

#[test]
fn two_links_inside_the_target_free_their_bytes_once() {
    let fs = fixture().hard_link(&format!("{INNER}/again"), BLOB);
    let outcome = run(&fs, Sink::Delete, true).outcome;
    assert_eq!(outcome, Outcome::Removed(Size::Known(3 * 4096)));
}

#[test]
fn a_failed_trash_move_keeps_the_item_and_never_deletes_it() {
    let fs = fixture().trash_unavailable();
    let Run {
        outcome,
        tally,
        log,
    } = run(&fs, Sink::Trash, true);
    assert!(
        matches!(&outcome, Outcome::Failed(f) if f.cause == FailCause::TrashFailed),
        "{outcome:?}"
    );
    assert!(fs.removed().is_empty() && fs.trashed().is_empty());
    assert!(fs.exists(BLOB));
    assert_eq!(tally.failed, 1);
    let rows = String::from_utf8_lossy(&log.current);
    assert!(rows.contains("FAILED /Users/alice/Library/Caches/com.example.app (sink=trash size=0 reason=trash-failed)"), "{rows}");
}

#[test]
fn trashed_bytes_are_never_counted_as_freed() {
    let fs = fixture();
    let Run { outcome, tally, .. } = run(&fs, Sink::Trash, true);
    assert_eq!(outcome, Outcome::Trashed(Size::Known(8192)));
    assert_eq!(tally.freed, Size::Known(0));
    assert_eq!(tally.moved_to_trash, Size::Known(8192));
    assert_eq!(fs.trashed(), [path(CACHE)]);
    assert!(fs.removed().is_empty());
}

#[test]
fn a_trash_move_of_a_swapped_item_is_kept() {
    let fs = fixture();
    fs.before_sink(|fs| fs.swap(CACHE));
    assert_eq!(run(&fs, Sink::Trash, true).outcome, changed());
    assert!(fs.trashed().is_empty());
}

#[test]
fn preview_refuses_exactly_what_apply_refuses_and_removes_nothing() {
    // SAFE-18: the same plan, two modes, the same refusals and reasons.
    let unsafe_trees = [
        fixture().mount(&format!("{INNER}/mnt"), 2),
        fixture().dataless(&format!("{INNER}/cloud")),
        fixture().unreadable(&format!("{INNER}/locked")),
        fixture().file(&format!("{INNER}/x/.claude/projects/session.jsonl")),
    ];
    for fs in unsafe_trees {
        let preview = run(&fs, Sink::Delete, false).outcome;
        assert!(matches!(preview, Outcome::Kept(_)), "{preview:?}");
        let apply = run(&fs, Sink::Delete, true).outcome;
        assert_eq!(preview, apply);
        assert!(fs.removed().is_empty());
    }
    let fs = fixture();
    assert_eq!(
        run(&fs, Sink::Delete, false).outcome,
        Outcome::WouldRemove(Size::Known(8192))
    );
    assert!(fs.removed().is_empty() && fs.exists(CACHE));
}

#[test]
fn a_preview_logs_its_refusals_in_a_dry_run_session() {
    let fs = fixture().mount(&format!("{INNER}/mnt"), 2);
    let log = run(&fs, Sink::Delete, false).log;
    let history = History::read(b"", &log.current);
    let session = history.sessions(Limit::DEFAULT).next().unwrap();
    assert_eq!(session.command, "test-dry-run");
    assert_eq!(session.actions.skipped, 1);
    let record = history.deletions(Limit::DEFAULT).next().unwrap();
    assert_eq!(record.status, "holds-kept");
}

#[test]
fn a_set_cancel_stops_before_the_recheck() {
    let fs = fixture();
    let guard = guard(&fs);
    let candidate = discover(&guard, &fs, CACHE, Sink::Delete);
    let clock = clock();
    let cancel = Cancel::default();
    let mut log = OpLog::new(MemoryStore::default(), |_: &_| {});
    let services = Services {
        guard: &guard,
        probe: &fs,
        clock: &clock,
        lease: Lease::BATCH,
        journal: &mut log,
        cancel: &cancel,
    };
    let run = RunId::new(clock.wall(), 1, 1);
    let mut executor = Executor::start(Mode::Apply(&fs), Command::named("test"), run, services);
    cancel.set();
    let calls = fs.node_calls();
    let (_, outcome) = executor.run(candidate);
    assert_eq!(outcome, Outcome::Cancelled);
    assert_eq!(fs.node_calls(), calls, "no probe ran after the signal");
    assert!(fs.exists(CACHE));
}

#[test]
fn a_log_that_cannot_be_written_warns_once_and_never_stops_a_delete() {
    let fs = fixture().dir(&format!("{CACHES}/second"));
    let guard = guard(&fs);
    let plan =
        [CACHE, &format!("{CACHES}/second")[..]].map(|at| discover(&guard, &fs, at, Sink::Delete));
    let clock = clock();
    let cancel = Cancel::default();
    let store = MemoryStore {
        failing: Some(std::io::ErrorKind::PermissionDenied),
        ..MemoryStore::default()
    };
    let mut warnings = Vec::new();
    let mut log = OpLog::new(store, |w: &_| warnings.push(Clone::clone(w)));
    let services = Services {
        guard: &guard,
        probe: &fs,
        clock: &clock,
        lease: Lease::BATCH,
        journal: &mut log,
        cancel: &cancel,
    };
    let run = RunId::new(clock.wall(), 1, 1);
    let mut executor = Executor::start(Mode::Apply(&fs), Command::named("test"), run, services);
    for candidate in plan {
        assert!(matches!(executor.run(candidate).1, Outcome::Removed(_)));
    }
    assert_eq!(executor.finish().removed, 2);
    drop(log);
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(!fs.exists(CACHE) && !fs.exists(&format!("{CACHES}/second")));
}

/// One candidate through a fresh executor whose cancel `arm` may set during the run.
fn run_signalled(
    fs: &FakeProbe,
    at: &str,
    sink: Sink,
    arm: impl FnOnce(&FakeProbe, Arc<Cancel>),
) -> Run {
    let guard = guard(fs);
    let clock = clock();
    let cancel = Arc::new(Cancel::default());
    let mut log = OpLog::new(MemoryStore::default(), |_: &_| {});
    let candidate = discover(&guard, fs, at, sink);
    arm(fs, Arc::clone(&cancel));
    let services = Services {
        guard: &guard,
        probe: fs,
        clock: &clock,
        lease: Lease::BATCH,
        journal: &mut log,
        cancel: &cancel,
    };
    let run = RunId::new(clock.wall(), 1, 1);
    let mut executor = Executor::start(Mode::Apply(fs), Command::named("test"), run, services);
    let (_, outcome) = executor.run(candidate);
    let tally = executor.finish();
    Run {
        outcome,
        tally,
        log: log.into_store(),
    }
}

#[test]
fn a_signal_during_the_recheck_of_a_file_stops_before_the_sink() {
    for sink in [Sink::Delete, Sink::Trash] {
        let fs = fixture();
        let ran = run_signalled(&fs, BLOB, sink, |fs, cancel| {
            fs.on_process_probe(move |_| cancel.set());
        });
        assert_eq!(ran.outcome, Outcome::Cancelled);
        assert!(fs.exists(BLOB));
        assert!(fs.removed().is_empty() && fs.trashed().is_empty());
    }
}

#[test]
fn a_signal_during_the_survey_stops_it_before_any_sink() {
    let empty = format!("{CACHES}/com.example.empty");
    for (at, sink) in [
        (CACHE, Sink::Delete),
        (empty.as_str(), Sink::Delete),
        (CACHE, Sink::Trash),
    ] {
        let fs = fixture().dir(&empty);
        let ran = run_signalled(&fs, at, sink, |fs, cancel| {
            fs.after_listing(at, move |_| cancel.set());
        });
        assert_eq!(ran.outcome, Outcome::Cancelled, "{at}");
        assert!(fs.exists(at));
        assert!(fs.removed().is_empty() && fs.trashed().is_empty());
    }
}

#[test]
fn a_trash_move_that_takes_another_object_logs_what_moved_and_where() {
    let fs = fixture();
    let ran = run_signalled(&fs, CACHE, Sink::Trash, |fs, _| {
        fs.during_trash(|fs| fs.swap(CACHE));
    });
    assert_eq!(
        ran.outcome,
        Outcome::TrashedOther(Moved {
            from: p(CACHE),
            to: p("/.Trash/com.example.app"),
        })
    );
    assert_eq!(ran.tally.failed, 1);
    assert_eq!(ran.tally.moved_to_trash, Size::Known(0));
    let rows = String::from_utf8_lossy(&ran.log.current);
    assert!(
        rows.contains("FAILED /Users/alice/Library/Caches/com.example.app (sink=trash size=0 reason=trashed-other)"),
        "{rows}"
    );
    assert!(
        rows.contains("TRASHED /Users/alice/Library/Caches/com.example.app (sink=trash size=unknown reason=trashed-other to=/.Trash/com.example.app)"),
        "{rows}"
    );
}

#[test]
fn no_live_probe_runs_after_a_signal_during_the_process_probe() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let at_signal = Arc::new(AtomicUsize::new(usize::MAX));
    let seen = Arc::clone(&at_signal);
    let fs = fixture();
    let ran = run_signalled(&fs, BLOB, Sink::Delete, move |fs, cancel| {
        fs.on_process_probe(move |fs| {
            cancel.set();
            seen.store(fs.open_file_calls(), Ordering::SeqCst);
        });
    });
    assert_eq!(ran.outcome, Outcome::Cancelled);
    assert_eq!(
        fs.open_file_calls(),
        at_signal.load(Ordering::SeqCst),
        "a snapshot of open files was taken after the signal"
    );
    assert!(fs.exists(BLOB));
}
