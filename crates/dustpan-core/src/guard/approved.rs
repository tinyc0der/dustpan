//! The one gate between a plan and a sink.
use super::Guard;
use super::open::OpenIndex;
use super::pass::{Evidence, Pass};
use super::survey::{self, Manifest};
use super::verdict::{ContentsReason, IdentityReason, KeptReason, ShapeReason, Verdict};
use crate::candidate::{Candidate, Sink};
use crate::identity::{NodeKind, PathIdentity};
use crate::path::AbsPath;
use crate::probe::{Owner, Probe, ProcessTable, Seen};
use crate::sink::Cancel;
use crate::time::Clock;
use std::sync::Arc;
use std::time::Duration;

/// How long one batch's live evidence (the process table and the snapshot of open files) may
/// answer for the candidates rechecked after it was taken.
///
/// One snapshot of each costs about 50 ms on the bench host, so taking them per candidate costs
/// 50 s per thousand. A per-candidate snapshot does not close the race either: an owner can
/// start while the sink runs, and a cache delete takes seconds. A lease adds at most its length
/// to that window. Evidence is taken at the sink, never during discovery. Each snapshot's age
/// starts from the monotonic time read before it was asked for and is checked at every
/// recheck, so a long sink call makes the next candidate take fresh evidence (SAFE-10). File
/// identity, spelling and every policy layer are never shared.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Lease(Duration);

impl Lease {
    /// Every recheck takes its own evidence. For the privileged helper (M4) and for tests.
    pub const NONE: Self = Self(Duration::ZERO);
    /// What a run of a reviewed plan uses.
    pub const BATCH: Self = Self(Duration::from_secs(1));

    pub const fn window(self) -> Duration {
        self.0
    }
}

/// A candidate the guard rechecked a moment ago, with everything it holds. The only input a
/// sink accepts.
///
/// The fields are private to this module and `Recheck::approve` is the only code here that
/// builds one, so no other module in any crate can make an `Approved`. It is not `Clone`, `Default` or
/// `Deserialize`: an approval cannot be copied, forged or received over a pipe. The privileged
/// helper (M4) receives candidates and runs its own `Recheck`, as root.
///
/// Forging one does not compile (private fields):
/// ```compile_fail,E0451
/// fn forge(c: dustpan_core::candidate::Candidate, i: dustpan_core::identity::PathIdentity) {
///     let _ = dustpan_core::guard::Approved { candidate: c, identity: i };
/// }
/// ```
/// Copying one does not compile:
/// ```compile_fail,E0599
/// fn copy(a: &dustpan_core::guard::Approved) -> dustpan_core::guard::Approved {
///     a.clone()
/// }
/// ```
/// Building a guard without loading its policy does not compile:
/// ```compile_fail,E0451
/// fn skip_policy() -> dustpan_core::guard::Guard {
///     dustpan_core::guard::Guard { policy: todo!() }
/// }
/// ```
#[derive(Debug)]
pub struct Approved {
    candidate: Candidate,
    identity: PathIdentity,
    /// The folders the survey judged. A sink enters no other folder.
    manifest: Manifest,
}

/// A candidate `recheck` did not approve, handed back so the executor can log and report it.
#[derive(Debug)]
pub struct Refused {
    pub candidate: Candidate,
    pub rejection: Rejection,
}

#[derive(Debug)]
pub enum Rejection {
    Kept(KeptReason),
    /// The path no longer exists. The sink contract counts this as done, not failed (M0-5).
    Gone,
    /// A signal came during the recheck. Nothing more was probed, and no sink runs (SAFE-20).
    Cancelled,
}

impl Approved {
    pub fn path(&self) -> &AbsPath {
        self.candidate.path()
    }

    /// The parent and target identity the sink compares against `fstat` of the parent it opens
    /// with `O_NOFOLLOW | O_DIRECTORY`, right before `unlinkat` (M0-5).
    pub fn identity(&self) -> PathIdentity {
        self.identity
    }

    pub fn sink(&self) -> &Sink {
        &self.candidate.rule().sink
    }

    pub fn candidate(&self) -> &Candidate {
        &self.candidate
    }

    /// For the outcome row after the sink ran, or in preview mode instead of the sink.
    pub fn into_candidate(self) -> Candidate {
        self.candidate
    }

    pub(crate) fn manifest(&self) -> &Manifest {
        &self.manifest
    }
}

/// One `lstat` each of `path` and its parent, with nothing remembered from before. The leaf's
/// kind tells the survey whether there is a tree to judge.
fn bind(path: &AbsPath, probe: &dyn Probe) -> Result<(PathIdentity, NodeKind), Rejection> {
    let unreadable =
        |at: AbsPath, why| Rejection::Kept(KeptReason::Shape(ShapeReason::Unreadable { at, why }));
    let node = |at: AbsPath| match probe.node(&at) {
        Seen::Found(node) => Ok(node),
        Seen::Missing => Err(Rejection::Gone),
        Seen::Unknown(why) => Err(unreadable(at, why)),
    };
    let target = node(path.clone())?;
    let parent = match path.parent() {
        Some(parent) => node(parent)?.id,
        None => target.id,
    };
    let identity = PathIdentity {
        parent,
        target: target.id,
    };
    Ok((identity, target.kind))
}

/// A snapshot and the monotonic time read before it was asked for.
struct Aged<T> {
    taken: Duration,
    value: Arc<T>,
}

/// One batch of rechecks at the sink. The executor holds one per run.
///
/// Each candidate gets a fresh pass, a fresh `lstat` of itself and its parent before and after
/// the live probes, and a fresh survey of what it holds. Only the live evidence is shared, and
/// only for the batch's [`Lease`]. Nothing from discovery is shared: discovery's passes are
/// gone, and its identity is only ever compared, never refreshed.
pub struct Recheck<'a> {
    guard: &'a Guard,
    probe: &'a dyn Probe,
    clock: &'a dyn Clock,
    lease: Lease,
    processes: Option<Aged<Seen<ProcessTable>>>,
    open: Option<Aged<OpenIndex>>,
    cancel: Option<&'a Cancel>,
}

impl Guard {
    /// Opens a batch of rechecks. Dry run and real run both use it; only the sink call differs
    /// (SAFE-18). The privileged helper (M4) opens its own, as root, from its own `Guard`.
    pub fn recheck<'a>(
        &'a self,
        probe: &'a dyn Probe,
        clock: &'a dyn Clock,
        lease: Lease,
    ) -> Recheck<'a> {
        Recheck {
            guard: self,
            probe,
            clock,
            lease,
            processes: None,
            open: None,
            cancel: None,
        }
    }
}

impl<'a> Recheck<'a> {
    /// Stops at the next step once `cancel` is set: no later listing, snapshot or approval
    /// (SAFE-20). The executor always sets it.
    #[must_use]
    pub fn stop_on(self, cancel: &'a Cancel) -> Self {
        Self {
            cancel: Some(cancel),
            ..self
        }
    }
}

impl Recheck<'_> {
    /// The batch's evidence still inside the lease when this check starts.
    fn fresh(&self, now: Duration) -> Evidence {
        let window = self.lease.window();
        let young = |taken: Duration| now.saturating_sub(taken) < window;
        Evidence {
            processes: self
                .processes
                .as_ref()
                .filter(|aged| young(aged.taken))
                .map(|aged| Arc::clone(&aged.value)),
            open: self
                .open
                .as_ref()
                .filter(|aged| young(aged.taken))
                .map(|aged| Arc::clone(&aged.value)),
        }
    }

    /// Keeps what a candidate's passes took for themselves, aged from before the check began.
    /// Evidence that was seeded keeps its own, older age.
    fn keep(&mut self, started: Duration, seeded: &Evidence, held: Evidence) {
        if seeded.processes.is_none()
            && let Some(value) = held.processes
        {
            self.processes = Some(Aged {
                taken: started,
                value,
            });
        }
        if seeded.open.is_none()
            && let Some(value) = held.open
        {
            self.open = Some(Aged {
                taken: started,
                value,
            });
        }
    }

    /// Runs every layer again on the candidate's path, re-probes its owners, judges the spelling
    /// the file system stores for the path, and compares the live parent and target identity
    /// with discovery's, before the live probes and after them (SAFE-03, SAFE-10, SAFE-11).
    /// Then judges everything a folder candidate holds (SAFE-27, SAFE-38). When the live
    /// evidence is older than the lease once the survey ends, it is taken again and the
    /// candidate and the owners its contents named are judged against it, so no approval rests
    /// on evidence older than one lease, the survey's own time included. Never refreshes the
    /// identity to approve a replacement.
    #[expect(
        clippy::result_large_err,
        reason = "both sides carry the candidate; boxing adds an allocation per candidate"
    )]
    pub fn approve(&mut self, candidate: Candidate) -> Result<Approved, Refused> {
        let judged = if self.cancelled() {
            Err(Rejection::Cancelled)
        } else {
            self.judge(&candidate)
        };
        match judged {
            Ok(manifest) => Ok(Approved {
                identity: candidate.identity(),
                candidate,
                manifest,
            }),
            Err(rejection) => Err(Refused {
                candidate,
                rejection,
            }),
        }
    }

    fn judge(&mut self, candidate: &Candidate) -> Result<Manifest, Rejection> {
        let policy = &self.guard.policy;
        let probe = self.probe;
        let started = self.clock.elapsed();
        let seeded = self.fresh(started);
        let taken = self.taken(&seeded).unwrap_or(started);
        let found = candidate.identity();
        let mut pass = Pass::with_evidence(policy, probe, seeded.clone());
        // The pass binds identity in layer 1, before layer 3 asks who has the path open.
        let (verdict, before) = pass.judge(candidate.path(), None, &candidate.rule().owners);
        let mut held = pass.evidence();
        let changed = |reason| Rejection::Kept(KeptReason::Identity(reason));
        let surveyed = match (verdict, before) {
            (Verdict::Kept(reason), _) => Err(Rejection::Kept(reason)),
            (Verdict::Eligible, None) => Err(Rejection::Gone),
            (Verdict::Eligible, Some(before)) if before != found => {
                Err(changed(IdentityReason::Changed))
            }
            // A swap while the owner probe ran leaves a path layer 1 never saw.
            (Verdict::Eligible, Some(_)) => match bind(candidate.path(), probe) {
                Ok((after, kind)) if after == found => {
                    let mut inside = Pass::contents(policy, probe, held);
                    let path = candidate.path();
                    let stop = self.cancel;
                    let manifest = survey::survey(
                        &mut inside,
                        probe,
                        self.clock,
                        stop,
                        path,
                        found.target,
                        kind,
                    );
                    held = inside.evidence();
                    manifest.map(|manifest| (manifest, kind, inside.scopes_asked().to_vec()))
                }
                Ok(_) => Err(changed(IdentityReason::ReplacedDuringCheck)),
                Err(rejection) => Err(rejection),
            },
        };
        self.keep(started, &seeded, held);
        let (manifest, kind, scopes) = surveyed?;
        // Evidence this check took itself is as new as the check, and a leaf has no survey
        // that could age it. Anything else is held to the lease at the end of the survey.
        let survey_ran = kind == NodeKind::Dir;
        let window = self.lease.window();
        if (survey_ran || taken < started) && self.clock.elapsed().saturating_sub(taken) >= window {
            self.refresh(candidate, &scopes)?;
        }
        Ok(manifest)
    }

    /// Takes the process table and the snapshot of open files again and judges the candidate
    /// against them: its owners, its scope's owner, and every file open at or below it. A
    /// candidate above a cache scope (a whole container) holds folders another owner names,
    /// so each scope owner the survey asked about is asked again too. The new evidence is the
    /// batch's from here on.
    fn refresh(
        &mut self,
        candidate: &Candidate,
        scopes: &[(Owner, AbsPath)],
    ) -> Result<(), Rejection> {
        if self.cancelled() {
            return Err(Rejection::Cancelled);
        }
        let started = self.clock.elapsed();
        let mut pass = Pass::with_evidence(&self.guard.policy, self.probe, Evidence::default());
        let (verdict, now) = pass.judge(candidate.path(), None, &candidate.rule().owners);
        let mut checked = match (verdict, now) {
            (Verdict::Kept(reason), _) => Err(Rejection::Kept(reason)),
            (Verdict::Eligible, None) => Err(Rejection::Gone),
            (Verdict::Eligible, Some(now)) if now != candidate.identity() => Err(Rejection::Kept(
                KeptReason::Identity(IdentityReason::ReplacedDuringCheck),
            )),
            (Verdict::Eligible, Some(_)) => Ok(()),
        };
        if checked.is_ok()
            && let Some((at, reason)) = scopes
                .iter()
                .find_map(|(owner, at)| pass.owner_refusal(owner).map(|r| (at, r)))
        {
            checked = Err(Rejection::Kept(KeptReason::Contents(
                ContentsReason::Holds {
                    at: at.clone(),
                    reason: Box::new(KeptReason::Live(reason)),
                },
            )));
        }
        let Evidence { processes, open } = pass.evidence();
        self.processes = processes.map(|value| Aged {
            taken: started,
            value,
        });
        self.open = open.map(|value| Aged {
            taken: started,
            value,
        });
        checked
    }

    /// When the oldest part of seeded evidence was taken.
    fn taken(&self, seeded: &Evidence) -> Option<Duration> {
        let processes = seeded
            .processes
            .as_ref()
            .and(self.processes.as_ref())
            .map(|aged| aged.taken);
        let open = seeded
            .open
            .as_ref()
            .and(self.open.as_ref())
            .map(|aged| aged.taken);
        processes.into_iter().chain(open).min()
    }

    fn cancelled(&self) -> bool {
        self.cancel.is_some_and(Cancel::is_set)
    }
}
