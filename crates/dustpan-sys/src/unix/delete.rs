//! The descriptor-relative calls macOS and Linux share: `openat`, `fstat`, directory reads and
//! `unlinkat`. One of the files SAFE-01 lets call a removal; `crates/dustpan/tests/seams.rs`
//! lists them.
#![expect(
    clippy::disallowed_methods,
    reason = "SAFE-01: the Delete sink's unlinkat, reached only through core's sealed Disk calls"
)]
use crate::disk::DirHandle;
use dustpan_core::disk::{DiskError, Unlink};
use dustpan_core::identity::{DirEntry, FileId, NodeKind};
use std::ffi::{CStr, CString, OsStr, OsString};
use std::io;
use std::os::fd::{AsRawFd as _, FromRawFd as _, OwnedFd, RawFd};
use std::os::unix::ffi::{OsStrExt as _, OsStringExt as _};

/// The one errno table. Core decides what each class means.
pub(crate) fn classify(e: &io::Error) -> DiskError {
    match e.raw_os_error() {
        Some(libc::ENOENT) => DiskError::NotFound,
        Some(libc::ELOOP | libc::ENOTDIR) => DiskError::NotAFolder,
        Some(libc::EACCES | libc::EPERM) => DiskError::Denied,
        Some(libc::EROFS) => DiskError::ReadOnly,
        Some(libc::EBUSY | libc::EXDEV) => DiskError::Busy,
        Some(libc::ENOTEMPTY | libc::EEXIST) => DiskError::NotEmpty,
        Some(libc::EDEADLK) => DiskError::Dataless,
        Some(code) => DiskError::Other(code),
        None => DiskError::Other(0),
    }
}

pub(crate) fn last_error() -> DiskError {
    classify(&io::Error::last_os_error())
}

/// A name or path as C wants it. A name read from a directory holds no NUL.
pub(crate) fn c_str(raw: &OsStr) -> Result<CString, DiskError> {
    CString::new(raw.as_bytes()).map_err(|_| DiskError::Other(libc::EINVAL))
}

pub(crate) fn kind_of(mode: libc::mode_t) -> NodeKind {
    match mode & libc::S_IFMT {
        libc::S_IFDIR => NodeKind::Dir,
        libc::S_IFLNK => NodeKind::Link,
        libc::S_IFREG => NodeKind::File,
        _ => NodeKind::Other,
    }
}

/// Wraps a descriptor `open` or `openat` just returned.
pub(crate) fn owned(fd: RawFd) -> Result<DirHandle, DiskError> {
    if fd < 0 {
        return Err(last_error());
    }
    // SAFETY: `fd` was just returned by the kernel and nothing else owns it.
    Ok(DirHandle(unsafe { OwnedFd::from_raw_fd(fd) }))
}

/// `openat(at, name, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)`: a link or a non-folder
/// under `name` fails with `ELOOP` or `ENOTDIR` and never opens what it points to.
pub(crate) fn open_dir(at: &DirHandle, name: &OsStr) -> Result<DirHandle, DiskError> {
    let name = c_str(name)?;
    let flags = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
    // SAFETY: `at` is an open descriptor and `name` is NUL-terminated.
    owned(unsafe { libc::openat(at.0.as_raw_fd(), name.as_ptr(), flags) })
}

/// `fstat` of an open folder.
pub(crate) fn id(dir: &DirHandle) -> Result<FileId, DiskError> {
    // SAFETY: a zeroed `stat` is a valid output buffer.
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    // SAFETY: `dir` is an open descriptor and `st` is writable.
    if unsafe { libc::fstat(dir.0.as_raw_fd(), &mut st) } != 0 {
        return Err(last_error());
    }
    #[allow(
        clippy::unnecessary_cast,
        reason = "dev_t and ino_t widths differ by OS"
    )]
    Ok(FileId::new(st.st_dev as u64, u128::from(st.st_ino as u64)))
}

/// The names in an open folder, `.` and `..` left out, read through a duplicate of its
/// descriptor so the handle stays open for the calls that follow.
pub(crate) fn names(dir: &DirHandle) -> Result<Vec<OsString>, DiskError> {
    // SAFETY: `dir` is an open descriptor; `dup` returns a new one or -1.
    let copy = unsafe { libc::dup(dir.0.as_raw_fd()) };
    if copy < 0 {
        return Err(last_error());
    }
    // SAFETY: `copy` is an open descriptor that `fdopendir` takes over, or that we close.
    let stream = unsafe { libc::fdopendir(copy) };
    if stream.is_null() {
        let e = last_error();
        // SAFETY: `fdopendir` failed, so `copy` is still ours to close.
        unsafe { libc::close(copy) };
        return Err(e);
    }
    // The duplicate shares the read position with `dir`, so start from the top.
    // SAFETY: `stream` is an open directory stream.
    unsafe { libc::rewinddir(stream) };
    let mut out = Vec::new();
    let result = loop {
        // SAFETY: errno is thread-local; clearing it tells end of stream from failure.
        unsafe { *errno() = 0 };
        // SAFETY: `stream` is open; the entry stays valid until the next `readdir`.
        let entry = unsafe { libc::readdir(stream) };
        if entry.is_null() {
            // SAFETY: errno is thread-local.
            let code = unsafe { *errno() };
            break if code == 0 {
                Ok(())
            } else {
                Err(classify(&io::Error::from_raw_os_error(code)))
            };
        }
        // SAFETY: `d_name` is NUL-terminated inside the entry.
        let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) }.to_bytes();
        if name != b"." && name != b".." {
            out.push(OsString::from_vec(name.to_vec()));
        }
    };
    // SAFETY: `stream` is open and closed once, which closes `copy`.
    unsafe { libc::closedir(stream) };
    result.map(|()| out)
}

#[cfg(target_os = "macos")]
unsafe fn errno() -> *mut libc::c_int {
    // SAFETY: always returns this thread's errno.
    unsafe { libc::__error() }
}

#[cfg(target_os = "linux")]
unsafe fn errno() -> *mut libc::c_int {
    // SAFETY: always returns this thread's errno.
    unsafe { libc::__errno_location() }
}

/// Every entry of an open folder, each read with `stat` relative to it. An entry removed
/// between the read and its `stat` is left out: it is already gone.
pub(crate) fn list(
    dir: &DirHandle,
    out: &mut Vec<DirEntry>,
    stat: fn(&DirHandle, &OsStr) -> Result<DirEntry, DiskError>,
) -> Result<(), DiskError> {
    for name in names(dir)? {
        match stat(dir, &name) {
            Ok(entry) => out.push(entry),
            Err(DiskError::NotFound) => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// `unlinkat(at, name, 0)` or `unlinkat(at, name, AT_REMOVEDIR)`.
pub(crate) fn unlink(at: &DirHandle, name: &OsStr, what: Unlink) -> Result<(), DiskError> {
    let c = c_str(name)?;
    let flag = match what {
        Unlink::NotDir => 0,
        Unlink::Dir => libc::AT_REMOVEDIR,
    };
    // SAFETY: `at` is open and `c` is NUL-terminated.
    if unsafe { libc::unlinkat(at.0.as_raw_fd(), c.as_ptr(), flag) } != 0 {
        return Err(last_error());
    }
    Ok(())
}
