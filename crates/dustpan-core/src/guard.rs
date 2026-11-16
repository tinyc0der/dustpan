//! The guard: one compiled policy, five layers in order, two entry points.
//!
//! - [`Pass`] judges paths during discovery and for `explain`, and shares ancestor work between
//!   paths that share ancestors.
//! - [`Guard::recheck`] judges one candidate again at the sink with a fresh pass, compares its
//!   identity before and after the live probes, and is the only code that builds an [`Approved`].
//!
//! Layers: 1 shape (spelling, aliases, linked ancestors, leaf links, mount points), 2 critical
//! roots, 3 live state, 4 protection policy, 5 whitelist. The first layer that refuses
//! decides, and its reason is data ([`KeptReason`]), never a string a caller parses. Identity
//! refusals come only from `recheck`.
mod approved;
mod open;
mod pass;
mod verdict;

pub use approved::{Approved, Refused, Rejection};
pub use pass::{Judgment, Pass};
pub use verdict::{
    Hit, How, IdentityReason, KeptReason, Kind, Layer, LiveReason, Reach, ShapeReason, Source,
    Verdict, WhitelistHit,
};

use crate::path::AbsPath;
use crate::paths::{Env, Os};
use crate::probe::{Probe, Unseen};
use crate::protect::Policy;
use crate::whitelist::Whitelist;
use std::fmt;

/// Everything a guard is built from. No `Default`: each field is a decision the caller makes.
pub struct GuardInputs<'a> {
    pub os: Os,
    /// The invoking user's home folder, physically resolved. Anchors every `~` row.
    pub home: AbsPath,
    /// Expands `%SystemRoot%`-style anchors. Env rows only add refusals.
    pub env: Env<'a>,
    pub whitelist: Whitelist,
}

/// A compiled policy for one OS and one user. Immutable and `Sync`: worker threads share one.
///
/// The only constructor is [`Guard::new`], and it fails instead of building a partial policy.
/// With no `Guard` there is no `Pass`, no `recheck` and no `Approved`, so a failed load refuses
/// every path by construction (SAFE-07).
pub struct Guard {
    policy: Policy,
}

#[derive(Debug)]
pub enum PolicyError {
    /// A table entry does not parse on this host. A bug in the table.
    BadAnchor(&'static str),
    /// A protected anchor exists but cannot be inspected, so its aliases cannot be refused.
    AnchorUnreadable { anchor: AbsPath, why: Unseen },
    /// The home folder is itself a critical root (`HOME=/`, `HOME=/Users`), so `~` rows would
    /// anchor on the wrong tree.
    HomeIsCritical(AbsPath),
}

impl fmt::Display for PolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadAnchor(row) => write!(f, "protection table entry {row:?} is not a valid path"),
            Self::AnchorUnreadable { anchor, why } => {
                write!(
                    f,
                    "cannot inspect protected path {anchor}: {}",
                    why.sentence()
                )
            }
            Self::HomeIsCritical(home) => {
                write!(f, "home folder {home} is a protected system path")
            }
        }
    }
}

impl Guard {
    /// Compiles the OS table, expands `~` and env anchors, and records the file identity of every
    /// protected anchor that exists. A few hundred `lstat` calls, once per process.
    pub fn new(inputs: GuardInputs<'_>, probe: &dyn Probe) -> Result<Self, PolicyError> {
        Ok(Self {
            policy: Policy::compile(&inputs, probe)?,
        })
    }

    #[cfg(all(test, unix))]
    pub(crate) fn from_policy(policy: Policy) -> Self {
        Self { policy }
    }

    /// Starts a pass. Use one per thread, and feed it paths in walk order so siblings share
    /// their ancestors' frames. Never carry a pass to the sink: `recheck` starts its own.
    pub fn pass<'a>(&'a self, probe: &'a dyn Probe) -> Pass<'a> {
        Pass::new(&self.policy, probe)
    }
}
