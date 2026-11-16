//! The call sites from `docs/design/m0-5-sinks.md`'s Usage section, type-checked: an M3 scan,
//! the M3 clean executor in preview and apply mode, and the M4 helper's journal. `history`
//! lives in `crates/dustpan/src/cmd/history.rs`, the live sink test in
//! `crates/dustpan-sys/tests/sink_fixture.rs`.
use dustpan_core::candidate::{Candidate, Rule};
use dustpan_core::disk::Disk;
use dustpan_core::guard::{Guard, Lease};
use dustpan_core::identity::Node;
use dustpan_core::oplog::{Command, Journal, Line, LogStore, LogWarning, OpLog, Row, RunId};
use dustpan_core::path::{AbsPath, Name};
use dustpan_core::probe::Probe;
use dustpan_core::sink::{Cancel, Executor, Mode, Outcome, Services, Tally};
use dustpan_core::time::Clock;
use std::sync::Arc;

/// M3: a catalog scan. One pass per worker. Entries arrive in walk order with the `Node` the
/// directory read reported, so siblings share their ancestors' frames and an entry costs no
/// guard syscall.
#[allow(dead_code)]
fn scan(
    guard: &Guard,
    probe: &dyn Probe,
    rule: &Arc<Rule>,
    root: &AbsPath,
    entries: &[(Name, Node)],
) -> Vec<Candidate> {
    let mut pass = guard.pass(probe);
    let mut plan = Vec::new();
    for (name, seen) in entries {
        match pass.candidate(root.join(name), *seen, rule) {
            Ok(candidate) => plan.push(candidate), // Kept candidates stay in the plan as review rows
            Err(vanished_or_unreadable) => drop(vanished_or_unreadable), // logged as SKIPPED in M3
        }
    }
    plan
}

/// M3 `clean`: the reviewed plan, in preview (`--dry-run`) or apply mode. The only branch on
/// `dry_run` is which `Mode` is built; everything after it is the same call (SAFE-18).
#[allow(dead_code, clippy::too_many_arguments)]
fn clean<D: Disk, S: LogStore>(
    guard: &Guard,
    probe: &dyn Probe,
    disk: &D,
    clock: &dyn Clock,
    store: S,
    cancel: &Cancel,
    plan: Vec<Candidate>,
    dry_run: bool,
    nonce: u64,
) -> (Vec<(Candidate, Outcome)>, Tally) {
    let mut log = OpLog::new(store, |w: &LogWarning| eprintln!("dustpan: {w}"));
    let mode = if dry_run {
        Mode::Preview
    } else {
        Mode::Apply(disk)
    };
    let run = RunId::new(clock.wall(), std::process::id(), nonce);
    let services = Services {
        guard,
        probe,
        clock,
        lease: Lease::BATCH,
        journal: &mut log,
        cancel,
    };
    let mut executor = Executor::start(mode, Command::CLEAN, run, services);
    let mut done = Vec::with_capacity(plan.len());
    for candidate in plan {
        done.push(executor.run(candidate)); // the TUI renders each as it comes
    }
    let tally = executor.finish();
    (done, tally)
}

/// M4: the privileged helper sends each item row to the parent, which owns the log file and the
/// session markers, so one session holds the user's and the helper's rows. The helper runs its
/// own `Executor` with `Lease::NONE` and this journal.
#[allow(dead_code)]
struct PipeJournal<W: std::io::Write> {
    pipe: W,
}

impl<W: std::io::Write> Journal for PipeJournal<W> {
    fn write(&mut self, line: Line<'_>) {
        if let Line::Item { row, .. } = line {
            let Row { action, path, .. } = row;
            // M4 serializes every field; the parent rebuilds the `Row` and writes it.
            let _ = writeln!(self.pipe, "{action:?} {path}");
        }
    }
}
