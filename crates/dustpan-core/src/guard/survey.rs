//! What a folder candidate holds, judged by the guard before an `Approved` exists.
//!
//! The discovery verdict judged the candidate's path, not the files inside it. A whole-folder
//! delete or move is refused when anything inside is kept (SAFE-27), a mount point inside is
//! never crossed (SAFE-38), and an online-only placeholder is never listed or removed. The
//! survey runs inside `recheck`, so a preview refuses exactly what a real run refuses (SAFE-18).
use super::approved::Rejection;
use super::pass::Pass;
use super::verdict::{Bound, ContentsReason, KeptReason, ShapeReason};
use crate::identity::{DirEntry, FileId, NodeKind};
use crate::path::AbsPath;
use crate::probe::{Probe, Seen, Unseen};
use crate::sink::Cancel;
use crate::time::Clock;
use std::time::Duration;

/// Folders nested below a candidate that the survey judges. The delete walk holds one handle per
/// level, so this also caps its open handles, and a deeper tree is refused before anything goes.
pub(crate) const MAX_DEPTH: usize = 64;
/// Entries one candidate may hold. About 3 s of listing and judging on the bench host.
pub(crate) const MAX_ENTRIES: u64 = 1_000_000;
/// The longest one survey may take. A survey that takes longer keeps the candidate (SAFE-20).
pub(crate) const DEADLINE: Duration = Duration::from_secs(30);

/// The folders the survey listed and judged. The delete walk enters only these, so a folder
/// moved into the tree after the survey is never entered: it stays, and the item ends
/// `changed-during-removal`. Memory is 16 bytes per folder, not per file.
#[derive(Debug)]
pub(crate) enum Manifest {
    /// A file, a link or another leaf. Nothing below it to judge.
    Leaf,
    /// A folder, its own id included in `dirs` (sorted file ids on `volume`).
    Tree { volume: u64, dirs: Box<[u128]> },
}

impl Manifest {
    pub(crate) fn admits(&self, dir: FileId) -> bool {
        match self {
            Self::Leaf => false,
            Self::Tree { volume, dirs } => {
                dir.volume() == *volume && dirs.binary_search(&dir.file()).is_ok()
            }
        }
    }
}

fn kept(reason: ContentsReason) -> Rejection {
    Rejection::Kept(KeptReason::Contents(reason))
}

fn holds(at: AbsPath, reason: KeptReason) -> Rejection {
    kept(ContentsReason::Holds {
        at,
        reason: Box::new(reason),
    })
}

fn unreadable(at: AbsPath, why: Unseen) -> KeptReason {
    KeptReason::Shape(ShapeReason::Unreadable { at, why })
}

/// Lists every folder below `root` through `Probe::list` and judges every entry with `pass`,
/// which shares the batch's live evidence and never follows a leaf link. The first kept entry
/// keeps the whole candidate. Depth first, so sibling entries share the pass's frames. A set
/// `cancel` stops it before the next listing (SAFE-20). The deadline is checked before and
/// after every listing and once more at the end, so no survey longer than it approves.
pub(super) fn survey(
    pass: &mut Pass<'_>,
    probe: &dyn Probe,
    clock: &dyn Clock,
    cancel: Option<&Cancel>,
    root: &AbsPath,
    target: FileId,
    kind: NodeKind,
) -> Result<Manifest, Rejection> {
    if kind != NodeKind::Dir {
        return Ok(Manifest::Leaf);
    }
    let deadline = clock.elapsed() + DEADLINE;
    let late = || {
        (clock.elapsed() > deadline)
            .then(|| kept(ContentsReason::Unbounded(Bound::Deadline(DEADLINE))))
    };
    let volume = target.volume();
    let mut dirs = vec![target.file()];
    let mut stack = vec![(root.clone(), target, 0)];
    let mut entries: Vec<DirEntry> = Vec::new();
    let mut seen: u64 = 0;
    while let Some((dir, expect, depth)) = stack.pop() {
        if cancel.is_some_and(Cancel::is_set) {
            return Err(Rejection::Cancelled);
        }
        if let Some(late) = late() {
            return Err(late);
        }
        entries.clear();
        let listed = probe.list(&dir, &mut entries);
        if let Some(late) = late() {
            return Err(late);
        }
        match listed {
            Seen::Found(id) if id == expect => {}
            // A folder that vanished or became another object while being listed: something is
            // writing in the tree, which is exactly when it must not be removed.
            Seen::Found(_) | Seen::Missing => {
                return Err(kept(ContentsReason::Changing { at: dir }));
            }
            Seen::Unknown(why) if dir == *root => {
                return Err(Rejection::Kept(unreadable(dir, why)));
            }
            Seen::Unknown(why) => return Err(holds(dir.clone(), unreadable(dir, why))),
        }
        seen += entries.len() as u64;
        if seen > MAX_ENTRIES {
            return Err(kept(ContentsReason::Unbounded(Bound::Entries(MAX_ENTRIES))));
        }
        for entry in entries.drain(..) {
            let Some(path) = dir.join_listed(&entry.name) else {
                return Err(holds(dir.clone(), unreadable(dir, Unseen::Failed)));
            };
            if entry.dataless {
                return Err(kept(ContentsReason::Dataless { at: path }));
            }
            if entry.mount_root || entry.node.id.volume() != volume {
                return Err(holds(path, KeptReason::Shape(ShapeReason::MountPoint)));
            }
            if let Some(reason) = pass.descendant(&path, entry.node) {
                return Err(holds(path, reason));
            }
            if entry.node.kind == NodeKind::Dir {
                if depth + 1 > MAX_DEPTH {
                    return Err(kept(ContentsReason::Unbounded(Bound::Depth(MAX_DEPTH))));
                }
                dirs.push(entry.node.id.file());
                stack.push((path, entry.node.id, depth + 1));
            }
        }
    }
    if let Some(late) = late() {
        return Err(late);
    }
    dirs.sort_unstable();
    Ok(Manifest::Tree {
        volume,
        dirs: dirs.into_boxed_slice(),
    })
}
