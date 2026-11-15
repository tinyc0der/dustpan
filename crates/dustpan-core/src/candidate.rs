//! What a scan found and what will happen to it.
use crate::guard::Verdict;
use crate::identity::PathIdentity;
use crate::path::AbsPath;
use crate::probe::Owner;
use crate::size::Size;
use std::sync::Arc;
use std::time::SystemTime;

/// A catalog row id such as `browser.chrome.cache`.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct RuleId(pub Box<str>);

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct GroupId(pub Box<str>);

/// How an approved candidate leaves the disk. A closed set: no other sink can be named.
#[derive(Debug)]
pub enum Sink {
    /// Rebuildable data only.
    Delete,
    /// Anything a person made or might want back.
    Trash,
    OwnerCommand(OwnerCommand),
    Privileged(PrivilegedAction),
}

/// `npm cache clean`, `brew cleanup`, `docker builder prune`. M3 adds the variants.
/// Uninhabited until then, so no M0 code can route a candidate to an owner command.
#[derive(Debug)]
pub enum OwnerCommand {}

/// The closed set the elevated helper runs. M4 adds the variants. Uninhabited until then.
#[derive(Debug)]
pub enum PrivilegedAction {}

/// The facts every candidate of one rule shares. M3's catalog loader builds one per TOML row and
/// may add fields. Analyze's Trash flow (M1) uses one fixed rule for user-selected paths.
///
/// Every rule has a sink. PLAN's `Sink::ReviewOnly` rows (large files, snapshots, iOS backups)
/// are shown and never touched, so M3 models them as planner rows that never become candidates,
/// and no sink or `Approved` has to reject one.
#[derive(Debug)]
pub struct Rule {
    pub id: RuleId,
    pub group: GroupId,
    /// Rechecked at the sink (SAFE-10). Kept here so `recheck` and the privileged helper need no
    /// catalog lookup.
    pub owners: Box<[Owner]>,
    pub sink: Sink,
    pub default_selected: bool,
}

/// One path a rule found, with the guard's discovery verdict and identity.
///
/// Only `guard::Pass::candidate` builds one, so every candidate was judged before any dry-run
/// branch (SAFE-18), and its verdict, identity and owners came from the same evaluation.
/// The detail-pane sentence is derived from `verdict` and the rule, not stored.
#[derive(Debug)]
pub struct Candidate {
    path: AbsPath,
    identity: PathIdentity,
    verdict: Verdict,
    rule: Arc<Rule>,
    pub size: Size,
    pub last_used: Option<SystemTime>,
}

impl Candidate {
    pub(crate) fn new(
        path: AbsPath,
        identity: PathIdentity,
        verdict: Verdict,
        rule: Arc<Rule>,
    ) -> Self {
        Self {
            path,
            identity,
            verdict,
            rule,
            size: Size::Unknown,
            last_used: None,
        }
    }

    pub fn path(&self) -> &AbsPath {
        &self.path
    }

    pub fn identity(&self) -> PathIdentity {
        self.identity
    }

    /// The discovery verdict. Informational: `guard::Guard::recheck` decides at the sink.
    pub fn verdict(&self) -> &Verdict {
        &self.verdict
    }

    pub fn rule(&self) -> &Rule {
        &self.rule
    }
}
