//! Linux parts of the Delete sink: the link-free open by path and the entry `statx`.
use crate::disk::DirHandle;
use crate::unix::delete::{c_str, kind_of, last_error, owned};
use dustpan_core::disk::DiskError;
use dustpan_core::identity::{DirEntry, FileId, Node};
use dustpan_core::path::AbsPath;
use std::ffi::OsStr;
use std::os::fd::AsRawFd as _;

/// Opens `path` one component at a time from `/`, each with `O_NOFOLLOW | O_DIRECTORY`, so a
/// link anywhere fails with `ELOOP` or `ENOTDIR`. Folders on the way open with `O_PATH`, which
/// needs no read permission; only the last opens for reading.
pub(crate) fn open_path(path: &AbsPath) -> Result<DirHandle, DiskError> {
    let root = c_str(OsStr::new("/"))?;
    let flags = libc::O_PATH | libc::O_DIRECTORY | libc::O_CLOEXEC;
    // SAFETY: `root` is NUL-terminated.
    let mut dir = owned(unsafe { libc::open(root.as_ptr(), flags) })?;
    let names: Vec<&OsStr> = path
        .as_path()
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(name) => Some(name),
            _ => None,
        })
        .collect();
    for (i, name) in names.iter().enumerate() {
        let last = i + 1 == names.len();
        let access = if last { libc::O_RDONLY } else { libc::O_PATH };
        let c = c_str(name)?;
        let flags = access | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
        // SAFETY: `dir` is open and `c` is NUL-terminated.
        dir = owned(unsafe { libc::openat(dir.0.as_raw_fd(), c.as_ptr(), flags) })?;
    }
    if names.is_empty() {
        // SAFETY: `root` is NUL-terminated.
        dir = owned(unsafe {
            libc::open(
                root.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
            )
        })?;
    }
    Ok(dir)
}

/// `statx(at, name, AT_SYMLINK_NOFOLLOW | AT_STATX_DONT_SYNC)`, which also says whether the
/// entry is the root of a mount: a bind mount shares its parent's device, so the volume
/// comparison alone misses it (SAFE-38).
pub(crate) fn stat(at: &DirHandle, name: &OsStr) -> Result<DirEntry, DiskError> {
    let c = c_str(name)?;
    // SAFETY: a zeroed `statx` is a valid output buffer.
    let mut st: libc::statx = unsafe { std::mem::zeroed() };
    let flags = libc::AT_SYMLINK_NOFOLLOW | libc::AT_STATX_DONT_SYNC;
    let mask = libc::STATX_TYPE | libc::STATX_INO | libc::STATX_NLINK | libc::STATX_BLOCKS;
    // SAFETY: `at` is open, `c` is NUL-terminated and `st` is writable.
    if unsafe { libc::statx(at.0.as_raw_fd(), c.as_ptr(), flags, mask, &mut st) } != 0 {
        return Err(last_error());
    }
    let mount_root = libc::STATX_ATTR_MOUNT_ROOT as u64;
    let dev = libc::makedev(st.stx_dev_major, st.stx_dev_minor);
    Ok(DirEntry {
        name: name.to_os_string(),
        node: Node {
            kind: kind_of(libc::mode_t::from(st.stx_mode)),
            id: FileId::new(dev, u128::from(st.stx_ino)),
        },
        alloc: st.stx_blocks.saturating_mul(512),
        links: st.stx_nlink,
        mount_root: st.stx_attributes_mask & mount_root != 0 && st.stx_attributes & mount_root != 0,
        dataless: false,
    })
}
