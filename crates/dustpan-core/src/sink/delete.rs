//! The Delete sink: a handle-relative, deepest-first walk over what the survey judged.
//!
//! Identity binding, from the top:
//! 1. The candidate's parent is opened with no link in any component (`Disk::open_parent`), and
//!    its `fstat` must equal the approved parent identity. That binds the whole ancestor chain
//!    at one instant: the path resolved, link-free, to the folder discovery recorded. From then
//!    on the handle is bound to that folder object, wherever it is renamed to.
//! 2. A folder candidate is opened relative to that handle with `O_NOFOLLOW | O_DIRECTORY`, and
//!    its `fstat` must equal the approved target. A file candidate is `fstatat`'d relative to
//!    the parent handle and unlinked by name right after: the narrowest window `unlinkat` by
//!    name allows.
//! 3. Every folder below is opened relative to its parent's handle, must be on the candidate's
//!    volume and not a mount root, and must be a folder the survey judged
//!    (`Manifest::admits`). A folder that fails any check is left, with everything in it.
//! 4. Files inside a checked folder are unlinked by name relative to its handle. A file that
//!    arrived after the survey is inside a folder the guard judged, as with `rm -rf`.
//! 5. Folders go with `AT_REMOVEDIR`, which removes only an empty folder: a folder swapped in
//!    under a name at that moment loses nothing.
use super::{Cancel, FailCause, Failure, Outcome};
use crate::candidate::Candidate;
use crate::disk::{Disk, DiskError, Seal, Unlink};
use crate::guard::{Approved, IdentityReason, KeptReason, MAX_DEPTH, Manifest};
use crate::identity::{DirEntry, FileId, NodeKind};
use crate::path::AbsPath;
use crate::size::Size;
use std::collections::HashMap;

fn changed(approved: Approved) -> (Candidate, Outcome) {
    let outcome = Outcome::Kept(KeptReason::Identity(IdentityReason::Changed));
    (approved.into_candidate(), outcome)
}

fn gone(approved: Approved) -> (Candidate, Outcome) {
    (approved.into_candidate(), Outcome::Gone)
}

fn failed(
    approved: Approved,
    cause: FailCause,
    at: Option<AbsPath>,
    freed: u64,
) -> (Candidate, Outcome) {
    let at = at.unwrap_or_else(|| approved.path().clone());
    let failure = Failure {
        cause,
        at,
        freed: Size::Known(freed),
    };
    (approved.into_candidate(), Outcome::Failed(failure))
}

pub(super) fn delete<D: Disk>(
    approved: Approved,
    disk: &D,
    cancel: &Cancel,
) -> (Candidate, Outcome) {
    let seal = Seal::new();
    let identity = approved.identity();
    let Some(name) = approved.path().name().map(ToOwned::to_owned) else {
        return changed(approved);
    };
    let parent = match disk.open_parent(approved.path(), &seal) {
        Ok(dir) => dir,
        Err(DiskError::NotFound) => return gone(approved),
        Err(DiskError::NotAFolder) => return changed(approved),
        Err(e) => return failed(approved, FailCause::of(e), None, 0),
    };
    if disk.id(&parent, &seal) != Ok(identity.parent) {
        return changed(approved);
    }
    let volume = match approved.manifest() {
        Manifest::Leaf => {
            let entry = match disk.stat(&parent, &name, &seal) {
                Ok(entry) if entry.node.id == identity.target => entry,
                Ok(_) => return changed(approved),
                Err(DiskError::NotFound) => return gone(approved),
                Err(e) => return failed(approved, FailCause::of(e), None, 0),
            };
            return match disk.unlink(&parent, &name, Unlink::NotDir, &seal) {
                // Another link elsewhere keeps the bytes, so nothing was freed (SAFE-21).
                Ok(()) => {
                    let freed = if entry.links <= 1 { entry.alloc } else { 0 };
                    (
                        approved.into_candidate(),
                        Outcome::Removed(Size::Known(freed)),
                    )
                }
                Err(DiskError::NotFound) => gone(approved),
                Err(e) => failed(approved, FailCause::of(e), None, 0),
            };
        }
        Manifest::Tree { volume, .. } => *volume,
    };
    let root = match disk.open_dir(&parent, &name, &seal) {
        Ok(root) => root,
        Err(DiskError::NotFound) => return gone(approved),
        Err(DiskError::NotAFolder) => return changed(approved),
        Err(e) => return failed(approved, FailCause::of(e), None, 0),
    };
    if disk.id(&root, &seal) != Ok(identity.target) {
        return changed(approved);
    }
    let mut walk = Walk {
        disk,
        seal: &seal,
        manifest: approved.manifest(),
        cancel,
        volume,
        freed: 0,
        links: HashMap::new(),
        stopped: None,
    };
    walk.empty(&root, approved.path(), 0);
    drop(root);
    let (freed, stopped) = (walk.freed, walk.stopped);
    if let Some((cause, at)) = stopped {
        return failed(approved, cause, Some(at), freed);
    }
    if cancel.is_set() {
        return failed(approved, FailCause::Interrupted, None, freed);
    }
    match disk.unlink(&parent, &name, Unlink::Dir, &seal) {
        Ok(()) | Err(DiskError::NotFound) => (
            approved.into_candidate(),
            Outcome::Removed(Size::Known(freed)),
        ),
        Err(e) => failed(approved, FailCause::of(e), None, freed),
    }
}

struct Walk<'a, D: Disk> {
    disk: &'a D,
    seal: &'a Seal,
    manifest: &'a Manifest,
    cancel: &'a Cancel,
    volume: u64,
    freed: u64,
    /// Links removed so far per multiply-linked file. Its bytes count when the last one goes,
    /// so a hard link that also lives outside the candidate frees nothing (SAFE-21).
    links: HashMap<FileId, u32>,
    /// The first entry the walk had to leave, and why.
    stopped: Option<(FailCause, AbsPath)>,
}

impl<D: Disk> Walk<'_, D> {
    fn stop(&mut self, cause: FailCause, at: AbsPath) {
        self.stopped.get_or_insert((cause, at));
    }

    /// Removes everything inside `dir`, deepest first. An entry it must leave is recorded and its
    /// siblings still go: the data is rebuildable, and the run reports the item failed with the
    /// bytes it did free.
    fn empty(&mut self, dir: &D::Dir, at: &AbsPath, depth: usize) {
        let mut entries = Vec::new();
        if let Err(e) = self.disk.list(dir, &mut entries, self.seal) {
            self.stop(FailCause::of(e), at.clone());
            return;
        }
        for entry in entries {
            if self.cancel.is_set() {
                self.stop(FailCause::Interrupted, at.clone());
                return;
            }
            let Some(path) = at.join_listed(&entry.name) else {
                self.stop(FailCause::Changed, at.clone());
                continue;
            };
            if entry.node.kind != NodeKind::Dir {
                self.remove(dir, &entry, Unlink::NotDir, path);
                continue;
            }
            // The survey refused deeper trees, so only a folder moved down after it reaches
            // this depth.
            if entry.mount_root
                || entry.node.id.volume() != self.volume
                || !self.manifest.admits(entry.node.id)
                || depth >= MAX_DEPTH
            {
                self.stop(FailCause::Changed, path);
                continue;
            }
            let child = match self.disk.open_dir(dir, &entry.name, self.seal) {
                Ok(child) => child,
                Err(DiskError::NotFound) => continue,
                Err(e) => {
                    self.stop(FailCause::of(e), path);
                    continue;
                }
            };
            if self.disk.id(&child, self.seal) != Ok(entry.node.id) {
                self.stop(FailCause::Changed, path);
                continue;
            }
            self.empty(&child, &path, depth + 1);
            drop(child);
            if self.cancel.is_set() {
                return;
            }
            self.remove(dir, &entry, Unlink::Dir, path);
        }
    }

    fn remove(&mut self, dir: &D::Dir, entry: &DirEntry, what: Unlink, path: AbsPath) {
        match self.disk.unlink(dir, &entry.name, what, self.seal) {
            Ok(()) => self.count(entry),
            Err(DiskError::NotFound) => {}
            Err(e) => self.stop(FailCause::of(e), path),
        }
    }

    fn count(&mut self, entry: &DirEntry) {
        // A folder's link count is its subfolders, not other names for it.
        if entry.links <= 1 || entry.node.kind == NodeKind::Dir {
            self.freed += entry.alloc;
            return;
        }
        let removed = self.links.entry(entry.node.id).or_insert(0);
        *removed += 1;
        if *removed == entry.links {
            self.freed += entry.alloc;
        }
    }
}
