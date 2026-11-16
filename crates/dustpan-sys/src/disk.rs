//! [`LiveDisk`]: core's `Disk` over real folder handles. Holds nothing between calls.
//!
//! Its methods take a `dustpan_core::disk::Seal`, which only core builds, so the binary can hold
//! a `LiveDisk` and still never call one: it hands the disk to `sink::Executor` in apply mode.
use dustpan_core::disk::{Disk, DiskError, Seal, Trashed, Unlink};
use dustpan_core::identity::{DirEntry, FileId};
use dustpan_core::path::AbsPath;
use dustpan_core::probe::{Seen, Unseen};
use std::ffi::OsStr;

#[cfg(target_os = "linux")]
use crate::linux::{delete as os_delete, trash as os_trash};
#[cfg(target_os = "macos")]
use crate::macos::{delete as os_delete, trash as os_trash};
#[cfg(unix)]
use crate::unix::delete as at;
#[cfg(windows)]
use crate::windows::{delete as os_delete, delete as at, trash as os_trash};

#[derive(Default)]
pub struct LiveDisk {
    _private: (),
}

impl LiveDisk {
    pub fn new() -> Self {
        Self::default()
    }
}

/// An open folder. Closes on drop.
#[cfg(unix)]
pub struct DirHandle(pub(crate) std::os::fd::OwnedFd);
#[cfg(windows)]
pub struct DirHandle(pub(crate) std::os::windows::io::OwnedHandle);

/// `Probe::list`: open by path with no link in any component, then the listing the walk uses.
/// The folder's own id comes from `fstat` of the descriptor that was listed.
pub(crate) fn list_path(dir: &AbsPath, out: &mut Vec<DirEntry>) -> Seen<FileId> {
    let unseen = |e: DiskError| match e {
        DiskError::NotFound => Seen::Missing,
        DiskError::Denied => Seen::Unknown(Unseen::Denied),
        _ => Seen::Unknown(Unseen::Failed),
    };
    let listed = os_delete::open_path(dir).and_then(|handle| {
        let id = at::id(&handle)?;
        at::list(&handle, out, os_delete::stat)?;
        Ok(id)
    });
    match listed {
        Ok(id) => Seen::Found(id),
        Err(e) => unseen(e),
    }
}

impl Disk for LiveDisk {
    type Dir = DirHandle;

    fn open_parent(&self, path: &AbsPath, _: &Seal) -> Result<DirHandle, DiskError> {
        match path.parent() {
            Some(parent) => os_delete::open_path(&parent),
            None => Err(DiskError::NotAFolder),
        }
    }

    fn open_dir(&self, dir: &DirHandle, name: &OsStr, _: &Seal) -> Result<DirHandle, DiskError> {
        at::open_dir(dir, name)
    }

    fn id(&self, dir: &DirHandle, _: &Seal) -> Result<FileId, DiskError> {
        at::id(dir)
    }

    fn stat(&self, dir: &DirHandle, name: &OsStr, _: &Seal) -> Result<DirEntry, DiskError> {
        os_delete::stat(dir, name)
    }

    fn list(&self, dir: &DirHandle, out: &mut Vec<DirEntry>, _: &Seal) -> Result<(), DiskError> {
        at::list(dir, out, os_delete::stat)
    }

    fn unlink(
        &self,
        dir: &DirHandle,
        name: &OsStr,
        what: Unlink,
        _: &Seal,
    ) -> Result<(), DiskError> {
        at::unlink(dir, name, what)
    }

    fn trash(
        &self,
        dir: &DirHandle,
        path: &AbsPath,
        expect: FileId,
        _: &Seal,
    ) -> Result<Trashed, DiskError> {
        os_trash::trash(dir, path, expect)
    }
}
