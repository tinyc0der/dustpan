//! macOS parts of the Delete sink: the link-free open by path and the entry `stat`.
use crate::disk::DirHandle;
use crate::unix::delete::{c_str, kind_of, last_error, owned};
use dustpan_core::disk::DiskError;
use dustpan_core::identity::{DirEntry, FileId, Node};
use dustpan_core::path::AbsPath;
use std::cell::Cell;
use std::ffi::OsStr;
use std::os::fd::AsRawFd as _;

/// `SF_DATALESS` from `<sys/stat.h>`: the file's data lives only in the cloud.
const SF_DATALESS: u32 = 0x4000_0000;
const IOPOL_TYPE_VFS_MATERIALIZE_DATALESS_FILES: libc::c_int = 3;
const IOPOL_SCOPE_THREAD: libc::c_int = 1;
const IOPOL_MATERIALIZE_DATALESS_FILES_OFF: libc::c_int = 1;

unsafe extern "C" {
    fn setiopolicy_np(iotype: libc::c_int, scope: libc::c_int, policy: libc::c_int) -> libc::c_int;
}

thread_local! {
    static NO_DOWNLOADS: Cell<bool> = const { Cell::new(false) };
}

/// Makes this thread's file calls fail with `EDEADLK` instead of downloading a dataless file or
/// folder (SAFE-38). Set once per thread, before the first open.
fn never_materialize() {
    NO_DOWNLOADS.with(|set| {
        if !set.get() {
            // SAFETY: plain integer arguments; the call only changes this thread's policy.
            let ok = unsafe {
                setiopolicy_np(
                    IOPOL_TYPE_VFS_MATERIALIZE_DATALESS_FILES,
                    IOPOL_SCOPE_THREAD,
                    IOPOL_MATERIALIZE_DATALESS_FILES_OFF,
                )
            } == 0;
            set.set(ok);
        }
    });
}

/// `open(path, O_RDONLY | O_DIRECTORY | O_NOFOLLOW_ANY | O_CLOEXEC)`. `O_NOFOLLOW_ANY`
/// (macOS 11) fails with `ELOOP` when any component, not only the last, is a link, so one call
/// binds the whole ancestor chain to "no link" at this instant.
pub(crate) fn open_path(path: &AbsPath) -> Result<DirHandle, DiskError> {
    never_materialize();
    let raw = c_str(path.as_path().as_os_str())?;
    let flags = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW_ANY | libc::O_CLOEXEC;
    // SAFETY: `raw` is NUL-terminated.
    owned(unsafe { libc::open(raw.as_ptr(), flags) })
}

/// `fstatat(at, name, AT_SYMLINK_NOFOLLOW)`. A mount inside shows as another `st_dev`, so
/// `mount_root` stays false here and the volume comparison catches it.
pub(crate) fn stat(at: &DirHandle, name: &OsStr) -> Result<DirEntry, DiskError> {
    let c = c_str(name)?;
    // SAFETY: a zeroed `stat` is a valid output buffer.
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    // SAFETY: `at` is open, `c` is NUL-terminated and `st` is writable.
    let failed = unsafe {
        libc::fstatat(
            at.0.as_raw_fd(),
            c.as_ptr(),
            &mut st,
            libc::AT_SYMLINK_NOFOLLOW,
        )
    } != 0;
    if failed {
        return Err(last_error());
    }
    Ok(DirEntry {
        name: name.to_os_string(),
        node: Node {
            kind: kind_of(st.st_mode),
            // The same widening `std`'s `MetadataExt::dev` does, so ids match `Probe::node`.
            id: FileId::new(st.st_dev as u64, u128::from(st.st_ino)),
        },
        alloc: u64::try_from(st.st_blocks).unwrap_or(0).saturating_mul(512),
        links: u32::from(st.st_nlink),
        mount_root: false,
        dataless: st.st_flags & SF_DATALESS != 0,
    })
}
