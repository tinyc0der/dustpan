//! The two future call sites from design.md's Usage section, type-checked against the sketch.
//! `Outcome`, `Mode` and `Sinks` are stand-ins for what M0-5 adds to core.
use dustpan_core::candidate::{Candidate, Rule};
use dustpan_core::guard::{Approved, Guard, KeptReason, Refused, Rejection};
use dustpan_core::identity::Node;
use dustpan_core::path::{AbsPath, Name};
use dustpan_core::probe::Probe;
use dustpan_core::size::Size;
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

#[allow(dead_code)]
enum Mode {
    Preview,
    Apply,
}

#[allow(dead_code)]
enum Outcome {
    Removed(Size),
    WouldRemove(Size),
    AlreadyGone,
    Skipped(KeptReason),
}

/// M0-5's sinks, implemented in dustpan-sys. Each takes an `Approved` by value: one approval,
/// one sink call.
#[allow(dead_code)]
trait Sinks {
    fn apply(&self, approved: Approved) -> (Candidate, Outcome);
}

/// M0-5: the executor. Dry run and real run share every step, including `recheck`; only the sink
/// call differs (SAFE-18). Cancellation is checked before each recheck (SAFE-20).
#[allow(dead_code)]
fn execute(
    guard: &Guard,
    probe: &dyn Probe,
    sinks: &dyn Sinks,
    plan: Vec<Candidate>,
    mode: Mode,
) -> Vec<(Candidate, Outcome)> {
    plan.into_iter()
        .map(|candidate| match guard.recheck(candidate, probe) {
            Ok(approved) => match mode {
                Mode::Apply => sinks.apply(approved),
                Mode::Preview => {
                    let candidate = approved.into_candidate();
                    let size = candidate.size;
                    (candidate, Outcome::WouldRemove(size))
                }
            },
            Err(Refused {
                candidate,
                rejection: Rejection::Gone,
            }) => (candidate, Outcome::AlreadyGone),
            Err(Refused {
                candidate,
                rejection: Rejection::Kept(reason),
            }) => (candidate, Outcome::Skipped(reason)),
        })
        .collect()
}
