//! The Trash sink. Moves the candidate whole; the survey already judged everything inside it
//! (SAFE-27 covers a move as well as a delete). A failure keeps the item where it is and is
//! never retried as a removal (SAFE-12): this module has no path to `Disk::unlink`.
use super::{FailCause, Failure, Outcome};
use crate::candidate::Candidate;
use crate::disk::{Disk, DiskError, Seal};
use crate::guard::{Approved, IdentityReason, KeptReason};
use crate::path::AbsPath;
use crate::size::Size;

pub(super) fn trash<D: Disk>(approved: Approved, disk: &D) -> (Candidate, Outcome) {
    let seal = Seal::new();
    let identity = approved.identity();
    let changed = || Outcome::Kept(KeptReason::Identity(IdentityReason::Changed));
    let failure = |cause, at: AbsPath| {
        Outcome::Failed(Failure {
            cause,
            at,
            freed: Size::Known(0),
        })
    };
    let parent = match disk.open_parent(approved.path(), &seal) {
        Ok(dir) => dir,
        Err(DiskError::NotFound) => return (approved.into_candidate(), Outcome::Gone),
        Err(DiskError::NotAFolder) => return (approved.into_candidate(), changed()),
        Err(e) => {
            let at = approved.path().clone();
            return (approved.into_candidate(), failure(cause(e), at));
        }
    };
    if disk.id(&parent, &seal) != Ok(identity.parent) {
        return (approved.into_candidate(), changed());
    }
    let outcome = match disk.trash(&parent, approved.path(), identity.target, &seal) {
        Ok(moved) if moved.arrived == identity.target => {
            Outcome::Trashed(approved.candidate().size)
        }
        // Something else reached the Trash. It is recoverable there, so say where.
        Ok(moved) => {
            let at =
                AbsPath::parse(moved.at.as_os_str()).unwrap_or_else(|_| approved.path().clone());
            failure(FailCause::TrashedOther, at)
        }
        Err(DiskError::NotFound) => Outcome::Gone,
        Err(DiskError::NotAFolder) => changed(),
        Err(e) => failure(cause(e), approved.path().clone()),
    };
    (approved.into_candidate(), outcome)
}

/// A folder `containermanagerd` owns refuses with a permission error (spike S2), and that is
/// reported as such. Every other refusal is `trash-failed`.
fn cause(error: DiskError) -> FailCause {
    match error {
        DiskError::Denied => FailCause::Denied,
        _ => FailCause::TrashFailed,
    }
}
