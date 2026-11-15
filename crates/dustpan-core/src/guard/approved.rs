//! The one gate between a plan and a sink.
use super::Guard;
use super::verdict::{IdentityReason, KeptReason, ShapeReason, Verdict};
use crate::candidate::{Candidate, Sink};
use crate::identity::PathIdentity;
use crate::path::AbsPath;
use crate::probe::{Probe, Seen};

/// A candidate the guard rechecked a moment ago. The only input a sink accepts.
///
/// The fields are private to this module and `Guard::recheck` is the only code here that builds
/// one, so no other module in any crate can make an `Approved`. It is not `Clone`, `Default` or
/// `Deserialize`: an approval cannot be copied, forged or received over a pipe. The privileged
/// helper (M4) receives candidates and runs `recheck` itself, as root.
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
}

/// One `lstat` each of `path` and its parent, with nothing remembered from before.
fn bind(path: &AbsPath, probe: &dyn Probe) -> Result<PathIdentity, Rejection> {
    let unreadable =
        |at: AbsPath, why| Rejection::Kept(KeptReason::Shape(ShapeReason::Unreadable { at, why }));
    let node = |at: AbsPath| match probe.node(&at) {
        Seen::Found(node) => Ok(node.id),
        Seen::Missing => Err(Rejection::Gone),
        Seen::Unknown(why) => Err(unreadable(at, why)),
    };
    let target = node(path.clone())?;
    let parent = match path.parent() {
        Some(parent) => node(parent)?,
        None => target,
    };
    Ok(PathIdentity { parent, target })
}

impl Guard {
    /// Runs every layer again on the candidate's path with a fresh pass, re-probes its owners,
    /// judges the spelling the file system stores for the path, and compares the live parent and
    /// target identity with discovery's, both before the live probes and after them (SAFE-03,
    /// SAFE-10, SAFE-11). Never refreshes the identity to approve a replacement.
    /// Dry run and real run both call this; only the sink call differs (SAFE-18).
    #[expect(
        clippy::result_large_err,
        reason = "both sides carry the candidate; boxing adds an allocation per candidate"
    )]
    pub fn recheck(&self, candidate: Candidate, probe: &dyn Probe) -> Result<Approved, Refused> {
        let found = candidate.identity();
        let mut pass = self.pass(probe);
        // The pass binds identity in layer 1, before layer 3 asks who has the path open.
        let (verdict, before) = pass.judge(candidate.path(), None, &candidate.rule().owners);
        let changed = |reason| Rejection::Kept(KeptReason::Identity(reason));
        let rejection = match (verdict, before) {
            (Verdict::Kept(reason), _) => Rejection::Kept(reason),
            (Verdict::Eligible, None) => Rejection::Gone,
            (Verdict::Eligible, Some(before)) if before != found => {
                changed(IdentityReason::Changed)
            }
            // A swap while the owner probe ran leaves a path layer 1 never saw.
            (Verdict::Eligible, Some(_)) => match bind(candidate.path(), probe) {
                Ok(after) if after == found => {
                    return Ok(Approved {
                        candidate,
                        identity: found,
                    });
                }
                Ok(_) => changed(IdentityReason::ReplacedDuringCheck),
                Err(rejection) => rejection,
            },
        };
        Err(Refused {
            candidate,
            rejection,
        })
    }
}
