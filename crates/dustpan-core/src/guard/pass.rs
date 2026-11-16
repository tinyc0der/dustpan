//! One discovery pass: judges many paths and shares the work their ancestors need.
use super::open::OpenIndex;
use super::verdict::{Hit, How, KeptReason, LiveReason, ShapeReason, Verdict};
use crate::candidate::{Candidate, Rule};
use crate::identity::{Node, NodeKind, PathIdentity};
use crate::path::AbsPath;
use crate::probe::{self, Owner, OwnerState, Probe, ProcessTable, Seen};
use crate::protect::{LiveChecks, Policy, ScopeOwner, Step};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

/// A stack of ancestor frames from the root down to the parent of the last path judged.
///
/// A new path pops the frames that are not its ancestors and pushes the missing ones, so paths
/// in depth-first order `lstat` each directory once. A scan hands each leaf's `Node` over from
/// its directory read, so a scanned candidate costs no leaf syscall at all; `evaluate` (for
/// `explain`) pays one leaf `lstat`. The pass also takes at most one process table and one
/// snapshot of open files, each on first need.
///
/// Everything a pass knows was observed after it started, and it is dropped before the sink:
/// `Guard::recheck` builds its own pass, so no discovery answer can approve a deletion.
pub struct Pass<'a> {
    policy: &'a Policy,
    probe: &'a dyn Probe,
    frames: Vec<Frame>,
    live: Live,
    scratch: Vec<u8>,
    /// False in the pass that judges a leaf link's target, which is already physical.
    follows_leaf_link: bool,
    /// False while the stored spelling of a path is judged, which has no other spelling.
    checks_spelling: bool,
}

/// What a pass has observed of the running system. Each part is taken once, on first need. The
/// pass that judges a link's target borrows it, so one pass costs one snapshot of each kind.
#[derive(Default)]
struct Live {
    processes: Option<Seen<ProcessTable>>,
    /// Owner states from the one process table, by bundle id and by app group.
    bundles: HashMap<Box<str>, OwnerState>,
    app_groups: HashMap<Box<str>, OwnerState>,
    open: Option<OpenIndex>,
}

/// What the pass learned about one ancestor directory.
struct Frame {
    path: AbsPath,
    state: Dir,
    step: Step,
    /// Whether no process holds anything open in this folder, once a path in it has asked.
    quiet: Option<bool>,
}

enum Dir {
    Found(Node),
    Missing,
    /// The first layer-1 refusal on this directory or one above it. Every descendant inherits it.
    Refused(ShapeReason),
}

fn refusal_for(owner: &Owner, state: OwnerState) -> Option<LiveReason> {
    match state {
        OwnerState::Idle => None,
        OwnerState::Running(pids) => Some(LiveReason::OwnerRunning {
            owner: owner.clone(),
            pids,
        }),
        OwnerState::Unknown(why) => Some(LiveReason::OwnerUnknown {
            owner: owner.clone(),
            why,
        }),
    }
}

/// One path's verdict, and the identity discovery saw (`None` when the path does not exist or
/// could not be read).
#[derive(Debug)]
pub struct Judgment {
    path: AbsPath,
    verdict: Verdict,
    identity: Option<PathIdentity>,
}

impl Judgment {
    pub fn path(&self) -> &AbsPath {
        &self.path
    }

    pub fn verdict(&self) -> &Verdict {
        &self.verdict
    }

    pub fn identity(&self) -> Option<PathIdentity> {
        self.identity
    }
}

impl<'a> Pass<'a> {
    pub(super) fn new(policy: &'a Policy, probe: &'a dyn Probe) -> Self {
        Self {
            policy,
            probe,
            frames: Vec::new(),
            live: Live::default(),
            scratch: Vec::new(),
            follows_leaf_link: true,
            checks_spelling: true,
        }
    }

    /// Judges a path with no rule owners, `lstat`ing it. `explain` uses this.
    pub fn evaluate(&mut self, path: AbsPath) -> Judgment {
        let (verdict, identity) = self.judge(&path, None, &[]);
        Judgment {
            path,
            verdict,
            identity,
        }
    }

    /// Judges a path a rule found, with the rule's owners, and builds the candidate from that one
    /// evaluation. `seen` is what the scanner's directory read reported for the entry
    /// (`getattrlistbulk`, `getdents64` plus `d_type`, `NtQueryDirectoryFile`), so the guard
    /// does not `lstat` the leaf again. That observation only shapes the discovery verdict:
    /// `recheck` takes its own before anything is approved. `Err` hands back the judgment when
    /// the parent could not be observed, so the scanner can still log the refusal (SAFE-22).
    #[expect(
        clippy::result_large_err,
        reason = "both sides carry the path and verdict; boxing adds an allocation per entry"
    )]
    pub fn candidate(
        &mut self,
        path: AbsPath,
        seen: Node,
        rule: &Arc<Rule>,
    ) -> Result<Candidate, Judgment> {
        let (verdict, identity) = self.judge(&path, Some(seen), &rule.owners);
        match identity {
            Some(identity) => Ok(Candidate::new(path, identity, verdict, Arc::clone(rule))),
            None => Err(Judgment {
                path,
                verdict,
                identity,
            }),
        }
    }

    /// Layers 1 to 4 in order; the first refusal decides. `seen` skips the leaf `lstat`.
    /// Layer 3 asks its last question, who holds the path open, after layer 4.
    ///
    /// A path that every layer passes is then judged under the spelling the file system stores
    /// for it, which can only refuse (SAFE-03). A scanned entry was named by a directory read,
    /// so it pays for that question only when its spelling could be an alias.
    pub(super) fn judge(
        &mut self,
        path: &AbsPath,
        seen: Option<Node>,
        owners: &[Owner],
    ) -> (Verdict, Option<PathIdentity>) {
        let canonical = self.policy.canonical(path);
        let path = &*canonical;
        let (verdict, identity) = self.judge_as_spelled(path, seen, owners);
        let asks = seen.is_none() || self.policy.may_be_alias(path);
        if verdict == Verdict::Eligible
            && identity.is_some()
            && asks
            && let Some(reason) = self.judge_stored_spelling(path)
        {
            return (Verdict::Kept(KeptReason::Shape(reason)), identity);
        }
        (verdict, identity)
    }

    fn judge_as_spelled(
        &mut self,
        path: &AbsPath,
        seen: Option<Node>,
        owners: &[Owner],
    ) -> (Verdict, Option<PathIdentity>) {
        let kept = |reason| (Verdict::Kept(reason), None);

        let parent = path.as_path().parent().map(|dir| self.enter(dir));
        let (parent_node, parent_missing) = match parent.map(|at| &self.frames[at].state) {
            Some(Dir::Refused(reason)) => return kept(KeptReason::Shape(reason.clone())),
            Some(Dir::Found(node)) => (Some(*node), false),
            Some(Dir::Missing) => (None, true),
            None => (None, false),
        };
        let leaf = match seen {
            Some(node) => Seen::Found(node),
            None => self.probe.node(path),
        };
        // volfs (`/.vol/<device>/<inode>`) names a file through folders `lstat` cannot see.
        if parent_missing && matches!(leaf, Seen::Found(_)) {
            let reason = self.judge_stored_spelling(path).unwrap_or_else(|| {
                let parent = path.parent().unwrap_or_else(|| path.clone());
                ShapeReason::ParentNotFound { parent }
            });
            return kept(KeptReason::Shape(reason));
        }
        let node = match leaf {
            Seen::Found(node) => Some(node),
            Seen::Missing => None,
            Seen::Unknown(why) => {
                let at = path.clone();
                return kept(KeptReason::Shape(ShapeReason::Unreadable { at, why }));
            }
        };
        let identity = node.map(|node| PathIdentity {
            parent: parent_node.unwrap_or(node).id,
            target: node.id,
        });
        if parent.is_some() && identity.is_some_and(|i| i.parent.volume() != i.target.volume()) {
            return (
                Verdict::Kept(KeptReason::Shape(ShapeReason::MountPoint)),
                identity,
            );
        }
        if node.is_some_and(|n| n.kind == NodeKind::Link)
            && self.follows_leaf_link
            && let Some(reason) = self.judge_link_target(path)
        {
            return (Verdict::Kept(KeptReason::Shape(reason)), identity);
        }

        let policy = self.policy;
        let id = node.map(|n| n.id);
        let ruling = match (parent, path.name()) {
            (Some(at), Some(name)) => {
                policy.rule(path, &self.frames[at].step, name, id, &mut self.scratch)
            }
            _ => policy.rule_root(path, id, &mut self.scratch),
        };
        if let Some(hit) = ruling.critical {
            return (Verdict::Kept(KeptReason::Critical(hit)), identity);
        }

        // The other live checks run before the protection rows and under every carve-out
        // (SAFE-06).
        let live = self.policy.live(path, &mut self.scratch);
        let mut refusal = live.powerlog.then_some(LiveReason::LiveDatabase);
        if refusal.is_none() {
            refusal = owners.iter().find_map(|owner| match owner {
                Owner::BundleId(id) => self.scope_refusal(ScopeOwner::Bundle(id)),
                Owner::AppGroup(folder) => self.scope_refusal(ScopeOwner::AppGroup(folder)),
                Owner::Process(_) => refusal_for(owner, owner.state_in(self.processes())),
            });
        }
        if refusal.is_none()
            && let Some(owner) = live.scope_owner
        {
            refusal = self.scope_refusal(owner);
        }
        if refusal.is_none() && live.edr {
            refusal = Some(LiveReason::EndpointSecurity);
        }
        if let Some(reason) = refusal {
            return (Verdict::Kept(KeptReason::Live(reason)), identity);
        }

        if let Some(hit) = ruling.protection {
            return (Verdict::Kept(KeptReason::Protected(hit)), identity);
        }
        for inside in ruling.inside {
            // Unknown counts as present: the state may be there.
            if inside.certain || !matches!(self.probe.node(&inside.inner), Seen::Missing) {
                let hit = Hit {
                    kind: inside.kind,
                    anchor: path.clone(),
                    how: How::Contains {
                        inner: inside.inner.clone(),
                    },
                };
                return (Verdict::Kept(KeptReason::Protected(hit)), identity);
            }
        }
        // Open files are asked about last. Every path that would be eligible reaches this
        // line, so no allowlist skips it (SAFE-06). A path kept above takes no snapshot and
        // keeps the reason that still holds after the app quits.
        match self.open_refusal(path, parent, &live) {
            Some(reason) => (Verdict::Kept(KeptReason::Live(reason)), identity),
            None => (Verdict::Eligible, identity),
        }
    }

    /// A leaf link is judged by where it points. The target can refuse the link, never grant it
    /// (SAFE-05). A dangling link has no target to protect.
    fn judge_link_target(&mut self, link: &AbsPath) -> Option<ShapeReason> {
        match probe::physical(self.probe, link) {
            Seen::Found(target) => {
                let mut pass = Pass::new(self.policy, self.probe);
                pass.follows_leaf_link = false;
                pass.live = std::mem::take(&mut self.live);
                let verdict = pass.judge(&target, None, &[]).0;
                self.live = pass.live;
                match verdict {
                    Verdict::Kept(reason) => Some(ShapeReason::LinkToProtected {
                        target,
                        layer: reason.layer(),
                    }),
                    Verdict::Eligible => None,
                }
            }
            Seen::Missing => None,
            Seen::Unknown(why) => Some(ShapeReason::Unreadable {
                at: link.clone(),
                why,
            }),
        }
    }

    /// The refusal the stored spelling of `path` earns, when it has another one. An alias can
    /// refuse a path and never grant it.
    fn judge_stored_spelling(&mut self, path: &AbsPath) -> Option<ShapeReason> {
        if !self.checks_spelling {
            return None;
        }
        let unreadable = |why| ShapeReason::Unreadable {
            at: path.clone(),
            why,
        };
        let stored = match self.probe.spelling(path) {
            Seen::Found(stored) => self.policy.canonical(&stored).into_owned(),
            // `lstat` found it a moment ago, so the two answers disagree.
            Seen::Missing => return Some(unreadable(probe::Unseen::Failed)),
            Seen::Unknown(why) => return Some(unreadable(why)),
        };
        if stored == *path {
            return None;
        }
        self.checks_spelling = false;
        let verdict = self.judge_as_spelled(&stored, None, &[]).0;
        self.checks_spelling = true;
        match verdict {
            Verdict::Kept(reason) => Some(ShapeReason::AliasOfKept {
                stored,
                layer: reason.layer(),
            }),
            Verdict::Eligible => None,
        }
    }

    /// Whether the app with this bundle id, or an app of this group, keeps the path. Asked
    /// once per name in a pass: every path in one cache folder shares the answer, and the
    /// table behind it does not change.
    fn scope_refusal(&mut self, scope: ScopeOwner<'_>) -> Option<LiveReason> {
        let (name, states) = match scope {
            ScopeOwner::Bundle(id) => (id, &self.live.bundles),
            ScopeOwner::AppGroup(folder) => (folder, &self.live.app_groups),
        };
        let known = states.get(name).cloned();
        if matches!(known, Some(OwnerState::Idle)) {
            return None;
        }
        let owner = match scope {
            ScopeOwner::Bundle(id) => Owner::BundleId(id.into()),
            ScopeOwner::AppGroup(folder) => Owner::AppGroup(folder.into()),
        };
        let state = known.unwrap_or_else(|| {
            let state = owner.state_in(self.processes());
            let states = match scope {
                ScopeOwner::Bundle(_) => &mut self.live.bundles,
                ScopeOwner::AppGroup(_) => &mut self.live.app_groups,
            };
            states.insert(name.into(), state.clone());
            state
        });
        refusal_for(&owner, state)
    }

    /// Leaves `self.frames` ending at `dir`, reusing every frame that is an ancestor of it.
    /// Returns the index of `dir`'s frame.
    fn enter(&mut self, dir: &Path) -> usize {
        let same = |frame: &Frame| frame.path.as_path().as_os_str() == dir.as_os_str();
        if !self.frames.last().is_some_and(same) {
            while self
                .frames
                .last()
                .is_some_and(|top| !dir.starts_with(top.path.as_path()))
            {
                self.frames.pop();
            }
            let mut missing: Vec<&Path> = dir.ancestors().collect();
            missing.truncate(missing.len() - self.frames.len());
            for path in missing.into_iter().rev() {
                self.push(AbsPath::trusted(path.to_path_buf()));
            }
        }
        self.frames.len() - 1
    }

    fn push(&mut self, path: AbsPath) {
        let parent = self.frames.last();
        let state = match parent.map(|p| &p.state) {
            Some(Dir::Refused(reason)) => Dir::Refused(reason.clone()),
            Some(Dir::Missing) => Dir::Missing,
            Some(Dir::Found(_)) | None => match self.probe.node(&path) {
                Seen::Found(node) if node.kind == NodeKind::Link => {
                    Dir::Refused(ShapeReason::LinkedAncestor { link: path.clone() })
                }
                Seen::Found(node) if node.kind == NodeKind::Dir => Dir::Found(node),
                Seen::Found(_) | Seen::Missing => Dir::Missing,
                Seen::Unknown(why) => Dir::Refused(ShapeReason::Unreadable {
                    at: path.clone(),
                    why,
                }),
            },
        };
        let id = match &state {
            Dir::Found(node) => Some(node.id),
            Dir::Missing | Dir::Refused(_) => None,
        };
        let step = match (parent, path.name()) {
            (Some(parent), Some(name)) => {
                self.policy.step(&parent.step, name, id, &mut self.scratch)
            }
            _ => self.policy.root_step(&path, id, &mut self.scratch),
        };
        self.frames.push(Frame {
            path,
            state,
            step,
            quiet: None,
        });
    }

    /// One process table per pass, taken the first time an owner needs it.
    fn processes(&mut self) -> &Seen<ProcessTable> {
        let probe = self.probe;
        self.live.processes.get_or_insert_with(|| probe.processes())
    }

    /// The direct sign that a path is live: a process holds it open, or a file anywhere below
    /// it, or a member of its SQLite family (SAFE-09). A folder named for a vendor or a daemon
    /// has no owner a name leads to, and this still keeps it.
    ///
    /// `parent` is the frame of the folder that holds `path`. A folder with nothing open in it
    /// answers for every path in it, and for a SQLite family, whose members share the folder.
    /// So a scan asks the snapshot once per folder and again only inside a folder that is held.
    fn open_refusal(
        &mut self,
        path: &AbsPath,
        parent: Option<usize>,
        live: &LiveChecks<'_>,
    ) -> Option<LiveReason> {
        let (probe, fold_case) = (self.probe, self.policy.folds_case());
        let open = &*self
            .live
            .open
            .get_or_insert_with(|| OpenIndex::new(probe.open_files(), fold_case));
        let scratch = &mut self.scratch;
        let quiet = parent.is_some_and(|at| {
            let folder = &mut self.frames[at];
            let path = &folder.path;
            *folder
                .quiet
                .get_or_insert_with(|| open.at_or_below(path, scratch).is_none())
        });
        let mut family = live.family.iter().flatten();
        let holder = if quiet {
            None
        } else {
            open.at_or_below(path, scratch)
                .or_else(|| family.find_map(|member| open.at(member, scratch)))
        };
        match holder {
            Some(held) => Some(LiveReason::InUse {
                pid: held.pid,
                file: AbsPath::trusted(held.path.clone()),
            }),
            None if live.family_unreadable => Some(LiveReason::InUseUnknown(probe::Unseen::Failed)),
            None => open.unknown().map(LiveReason::InUseUnknown),
        }
    }
}
