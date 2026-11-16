//! The macOS Trash sink: `NSFileManager trashItemAtURL:resultingItemURL:error:`, the method
//! spike S2 chose. It takes about 1 ms, picks a unique name itself, reports where the item
//! landed without listing `~/.Trash` (which privacy protection forbids), and needs no Finder or
//! Aqua session. One of the files SAFE-01 lets call a move; `crates/dustpan/tests/seams.rs`
//! lists them.
#![expect(
    clippy::disallowed_methods,
    reason = "SAFE-01: the Trash sink's move, reached only through core's sealed Disk calls"
)]
use super::delete::stat;
use crate::disk::DirHandle;
use dustpan_core::disk::{DiskError, Trashed};
use dustpan_core::identity::FileId;
use dustpan_core::path::AbsPath;
use objc2_foundation::{NSFileManager, NSURL};
use std::ffi::{CStr, CString, OsStr};
use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::fs::MetadataExt as _;
use std::path::PathBuf;
use std::ptr::NonNull;

/// `NSFileNoSuchFileError`.
const NO_SUCH_FILE: isize = 4;
/// `NSFileWriteNoPermissionError`: a container folder `containermanagerd` owns, or a privacy
/// refusal (spike S2).
const NO_PERMISSION: isize = 513;

/// Moves `path` to the Trash after checking, relative to its open parent, that the entry is
/// still `expect`. The move itself goes by path: an object swapped in during the microseconds
/// between the check and the move reaches the Trash, where it is recoverable, and its `FileId`
/// in `Trashed::arrived` tells core it was not the approved one. Never retried, never deleted
/// (SAFE-12).
pub(crate) fn trash(
    parent: &DirHandle,
    path: &AbsPath,
    expect: FileId,
) -> Result<Trashed, DiskError> {
    let name = path.name().ok_or(DiskError::NotAFolder)?;
    if stat(parent, name)?.node.id != expect {
        return Err(DiskError::NotAFolder);
    }
    let raw = CString::new(path.as_path().as_os_str().as_bytes())
        .map_err(|_| DiskError::Other(libc::EINVAL))?;
    // SAFETY: `raw` is a NUL-terminated file system path that outlives the call.
    let url = unsafe {
        NSURL::fileURLWithFileSystemRepresentation_isDirectory_relativeToURL(
            NonNull::from(raw.as_c_str()).cast(),
            false,
            None,
        )
    };
    let mut landed = None;
    let manager = NSFileManager::defaultManager();
    if let Err(error) = manager.trashItemAtURL_resultingItemURL_error(&url, Some(&mut landed)) {
        return Err(match error.code() {
            NO_SUCH_FILE => DiskError::NotFound,
            NO_PERMISSION => DiskError::Denied,
            _ => DiskError::NoTrash,
        });
    }
    let landed = landed.ok_or(DiskError::NoTrash)?;
    // SAFETY: a file URL's representation is a NUL-terminated path owned by `landed`.
    let at = unsafe { CStr::from_ptr(landed.fileSystemRepresentation().as_ptr()) };
    let at = PathBuf::from(OsStr::from_bytes(at.to_bytes()));
    let meta = std::fs::symlink_metadata(&at).map_err(|_| DiskError::NoTrash)?;
    Ok(Trashed {
        arrived: FileId::new(meta.dev(), u128::from(meta.ino())),
        at,
    })
}
