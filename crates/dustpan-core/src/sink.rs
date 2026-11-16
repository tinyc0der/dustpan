//! The executor and the two sinks: the only code in dustpan that removes or moves a file.
//!
//! [`Executor`] takes one candidate at a time from a reviewed plan. Each goes through the same
//! steps in both modes: the sticky cancel, `Recheck::approve` (every guard layer, identity
//! before and after the live probes, the survey of what a folder holds), then one row. Only the
//! step after approval differs (SAFE-18): [`Mode::Preview`] reports what would happen and holds
//! no [`Disk`], so a preview cannot remove anything by construction; [`Mode::Apply`] hands the
//! `Approved` to the sink its rule names.
use crate::candidate::{Candidate, Sink};
use crate::disk::{Disk, DiskError};
use crate::guard::{Guard, KeptReason, Lease, Recheck, Refused, Rejection};
use crate::oplog::{Action, Command, Detail, Journal, Line, Row, RunId, Session, SinkWord};
use crate::path::AbsPath;
use crate::probe::Probe;
use crate::size::Size;
use crate::time::Clock;
use std::sync::atomic::{AtomicBool, Ordering};

mod delete;
mod trash;

/// What the run does after a candidate is approved. The disk exists only in `Apply`.
pub enum Mode<'a, D> {
    Preview,
    Apply(&'a D),
}

/// What happened to one candidate. Preview yields `WouldRemove`, `WouldTrash`, `Gone`, `Kept`
/// and `Cancelled`; apply yields `Removed`, `Trashed`, `TrashedOther`, `Gone`, `Kept`, `Failed`
/// and `Cancelled`.
/// One enum serves both, so a report, a JSON item and a TUI row have one shape.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Outcome {
    /// The sizer's measurement of what removal would free.
    WouldRemove(Size),
    /// The sizer's measurement of what would move to the Trash. Never counted as freed.
    WouldTrash(Size),
    /// Allocated bytes the delete freed, counted file by file as each last link went.
    Removed(Size),
    /// Bytes moved to the Trash. Never counted as freed (SAFE-21).
    Trashed(Size),
    /// The path was not there at the recheck or at the sink. Success: the goal state holds.
    Gone,
    /// The recheck, its survey, or the sink's identity check refused. Nothing was touched.
    Kept(KeptReason),
    /// The sink started and could not finish.
    Failed(Failure),
    /// The Trash move took another object than the approved one: something replaced the
    /// candidate in its folder between the sink's check and the move. That object is in the
    /// Trash, recoverable; the candidate failed (`trashed-other`).
    TrashedOther(Moved),
    /// A signal came before this candidate's sink. No later probe or sink ran for it
    /// (SAFE-20).
    Cancelled,
}

/// A sink that stopped. `freed` is what a delete removed before it stopped: real, completed
/// work, so it counts (SAFE-21). A Trash failure frees nothing.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Failure {
    pub cause: FailCause,
    /// The entry the sink was working on: the candidate, or a path inside it.
    pub at: AbsPath,
    pub freed: Size,
}

/// What a Trash move moved that was not approved, and where it landed. Both are logged.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Moved {
    pub from: AbsPath,
    pub to: AbsPath,
}

/// Each cause has a stable code for the row and `--json` (SAFE-35).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FailCause {
    /// `permission-denied`. Includes a container folder `containermanagerd` owns (spike S2).
    Denied,
    /// `readonly-filesystem`.
    ReadOnly,
    /// `busy`: something is mounted on, or (Windows) holds open, an entry.
    Busy,
    /// `changed-during-removal`: a folder or entry appeared, or a folder was replaced, after the
    /// survey. It was left, so the candidate's folder is not empty.
    Changed,
    /// `interrupted`: a signal came during the walk. Later candidates are `Cancelled`.
    Interrupted,
    /// `trash-failed`: the Trash refused or does not exist. The item stays; it is never deleted
    /// instead (SAFE-12).
    TrashFailed,
    /// `io-error`.
    Io,
}

impl FailCause {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Denied => "permission-denied",
            Self::ReadOnly => "readonly-filesystem",
            Self::Busy => "busy",
            Self::Changed => "changed-during-removal",
            Self::Interrupted => "interrupted",
            Self::TrashFailed => "trash-failed",
            Self::Io => "io-error",
        }
    }

    pub(crate) fn of(error: DiskError) -> Self {
        match error {
            DiskError::Denied => Self::Denied,
            DiskError::ReadOnly => Self::ReadOnly,
            DiskError::Busy => Self::Busy,
            DiskError::NotEmpty | DiskError::NotAFolder | DiskError::Dataless => Self::Changed,
            DiskError::NoTrash | DiskError::InTrash => Self::TrashFailed,
            DiskError::NotFound | DiskError::Other(_) => Self::Io,
        }
    }
}

/// The sticky cancel (SAFE-20). The signal handler sets it; once set it stays set, the executor
/// approves nothing more, and a delete walk stops between two entries.
#[derive(Debug, Default)]
pub struct Cancel(AtomicBool);

impl Cancel {
    pub fn set(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_set(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// Counters over a run. `freed` and `moved_to_trash` are separate fields fed by separate
/// variants, so no code path adds Trash bytes to the freed total (SAFE-21).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Tally {
    pub removed: u64,
    pub trashed: u64,
    pub would_remove: u64,
    pub would_trash: u64,
    pub gone: u64,
    pub kept: u64,
    pub failed: u64,
    pub cancelled: u64,
    pub freed: Size,
    pub moved_to_trash: Size,
    pub would_free: Size,
}

impl Default for Tally {
    fn default() -> Self {
        Self {
            removed: 0,
            trashed: 0,
            would_remove: 0,
            would_trash: 0,
            gone: 0,
            kept: 0,
            failed: 0,
            cancelled: 0,
            freed: Size::Known(0),
            moved_to_trash: Size::Known(0),
            would_free: Size::Known(0),
        }
    }
}

impl Tally {
    fn add(&mut self, outcome: &Outcome) {
        match outcome {
            Outcome::WouldRemove(size) => {
                self.would_remove += 1;
                self.would_free = self.would_free.plus(*size);
            }
            Outcome::WouldTrash(_) => self.would_trash += 1,
            Outcome::Removed(size) => {
                self.removed += 1;
                self.freed = self.freed.plus(*size);
            }
            Outcome::Trashed(size) => {
                self.trashed += 1;
                self.moved_to_trash = self.moved_to_trash.plus(*size);
            }
            Outcome::Gone => self.gone += 1,
            Outcome::Kept(_) => self.kept += 1,
            Outcome::Failed(failure) => {
                self.failed += 1;
                self.freed = self.freed.plus(failure.freed);
            }
            Outcome::TrashedOther(_) => self.failed += 1,
            Outcome::Cancelled => self.cancelled += 1,
        }
    }

    /// The `items` of the session end marker: candidates that left their place.
    pub fn items(&self) -> u64 {
        self.removed + self.trashed
    }
}

/// What every run borrows. The binary, the sink integration test and the privileged helper
/// (M4) each build one from parts they can make themselves: their own `Guard`, `LiveProbe`,
/// `SystemClock`, journal and cancel flag. No `Default`: each field is a decision.
pub struct Services<'a> {
    pub guard: &'a Guard,
    pub probe: &'a dyn Probe,
    pub clock: &'a dyn Clock,
    /// [`Lease::BATCH`] for a reviewed plan, [`Lease::NONE`] for one fresh snapshot per item.
    pub lease: Lease,
    pub journal: &'a mut dyn Journal,
    pub cancel: &'a Cancel,
}

/// One run of a plan. The one door to a sink.
pub struct Executor<'a, D: Disk> {
    mode: Mode<'a, D>,
    session: Session,
    recheck: Recheck<'a>,
    clock: &'a dyn Clock,
    journal: &'a mut dyn Journal,
    cancel: &'a Cancel,
    tally: Tally,
}

impl<'a, D: Disk> Executor<'a, D> {
    /// Opens the session and writes its start marker: `<command>` in apply mode,
    /// `<command>-dry-run` in preview, named by the mode itself.
    pub fn start(mode: Mode<'a, D>, command: Command, run: RunId, services: Services<'a>) -> Self {
        let Services {
            guard,
            probe,
            clock,
            lease,
            journal,
            cancel,
        } = services;
        let session = Session::new(command, run, matches!(mode, Mode::Preview));
        journal.write(Line::Start {
            session: &session,
            at: clock.wall(),
        });
        Self {
            mode,
            session,
            recheck: guard.recheck(probe, clock, lease).stop_on(cancel),
            clock,
            journal,
            cancel,
            tally: Tally::default(),
        }
    }

    /// Takes one candidate through cancel, recheck, survey and, in apply mode, its sink, and
    /// writes its row. Feed candidates in plan order; the TUI renders each result as it comes.
    pub fn run(&mut self, candidate: Candidate) -> (Candidate, Outcome) {
        if self.cancel.is_set() {
            self.tally.add(&Outcome::Cancelled);
            return (candidate, Outcome::Cancelled);
        }
        let (candidate, outcome) = match self.recheck.approve(candidate) {
            Err(Refused {
                candidate,
                rejection: Rejection::Gone,
            }) => (candidate, Outcome::Gone),
            Err(Refused {
                candidate,
                rejection: Rejection::Kept(reason),
            }) => (candidate, Outcome::Kept(reason)),
            Err(Refused {
                candidate,
                rejection: Rejection::Cancelled,
            }) => (candidate, Outcome::Cancelled),
            // A signal during the recheck: the approval stands for nothing (SAFE-20).
            Ok(approved) if self.cancel.is_set() => (approved.into_candidate(), Outcome::Cancelled),
            Ok(approved) => match &self.mode {
                Mode::Preview => {
                    let candidate = approved.into_candidate();
                    let outcome = match &candidate.rule().sink {
                        Sink::Delete => Outcome::WouldRemove(candidate.size),
                        Sink::Trash => Outcome::WouldTrash(candidate.size),
                        Sink::OwnerCommand(never) => match *never {},
                        Sink::Privileged(never) => match *never {},
                    };
                    (candidate, outcome)
                }
                Mode::Apply(disk) => match approved.sink() {
                    Sink::Delete => delete::delete(approved, *disk, self.cancel),
                    Sink::Trash => trash::trash(approved, *disk, self.cancel),
                    Sink::OwnerCommand(never) => match *never {},
                    Sink::Privileged(never) => match *never {},
                },
            },
        };
        for row in rows(&candidate, &outcome, self.clock).into_iter().flatten() {
            self.journal.write(Line::Item {
                session: &self.session,
                row,
            });
        }
        self.tally.add(&outcome);
        (candidate, outcome)
    }

    /// Writes the end marker with the run's items and freed bytes, and returns the counters.
    /// A run dropped without `finish` leaves a session with no end marker, which `history`
    /// shows as not ended, the same as a crash.
    pub fn finish(self) -> Tally {
        self.journal.write(Line::End {
            session: &self.session,
            at: self.clock.wall(),
            items: self.tally.items(),
            freed: self.tally.freed,
        });
        self.tally
    }
}

/// The one map from an outcome to its rows. Every mutation and every refusal has one, in both
/// modes; a would-be removal and a cancelled candidate have none, since nothing happened and
/// nothing was judged. A `TrashedOther` has two: the candidate's failure, and the move of the
/// object that did reach the Trash, named where it was and where it went.
fn rows<'c>(
    candidate: &'c Candidate,
    outcome: &'c Outcome,
    clock: &dyn Clock,
) -> [Option<Row<'c>>; 2] {
    let sink = match &candidate.rule().sink {
        Sink::Delete => SinkWord::Delete,
        Sink::Trash => SinkWord::Trash,
        Sink::OwnerCommand(never) => match *never {},
        Sink::Privileged(never) => match *never {},
    };
    let detail = |size, reason, inner| Detail {
        sink: Some(sink),
        size,
        reason,
        inner,
        to: None,
    };
    let row = |action, path, detail| {
        Some(Row {
            at: clock.wall(),
            action,
            path,
            detail,
        })
    };
    let own = |action, detail| [row(action, candidate.path(), detail), None];
    match outcome {
        Outcome::WouldRemove(_) | Outcome::WouldTrash(_) | Outcome::Cancelled => [None, None],
        Outcome::Removed(size) => own(Action::Removed, detail(Some(*size), None, None)),
        Outcome::Trashed(size) => own(Action::Trashed, detail(Some(*size), None, None)),
        Outcome::Gone => own(Action::Skipped, detail(None, Some("gone"), None)),
        Outcome::Kept(reason) => own(
            Action::Skipped,
            detail(
                None,
                Some(reason.code()),
                reason.inner().map(KeptReason::code),
            ),
        ),
        Outcome::Failed(failure) => own(
            Action::Failed,
            detail(Some(failure.freed), Some(failure.cause.code()), None),
        ),
        Outcome::TrashedOther(moved) => [
            row(
                Action::Failed,
                candidate.path(),
                detail(Some(Size::Known(0)), Some(TRASHED_OTHER), None),
            ),
            row(
                Action::Trashed,
                &moved.from,
                Detail {
                    to: Some(&moved.to),
                    ..detail(Some(Size::Unknown), Some(TRASHED_OTHER), None)
                },
            ),
        ],
    }
}

/// The reason both rows of a `TrashedOther` carry.
pub const TRASHED_OTHER: &str = "trashed-other";
