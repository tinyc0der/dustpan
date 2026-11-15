use crate::probe::unseen;
use dustpan_core::path::AbsPath;
use dustpan_core::probe::{Seen, Unseen};
use std::ffi::{CString, OsStr};
use std::io;
use std::os::unix::ffi::OsStrExt as _;

/// What `getattrlist` writes for `ATTR_CMN_FULLPATH`: the total length, a reference to the
/// path, then the path's bytes.
#[repr(C)]
struct FullPath {
    length: u32,
    path: libc::attrreference_t,
    bytes: [u8; libc::PATH_MAX as usize],
}

/// The path the kernel builds for the file `path` names, from the names stored on disk. One
/// `getattrlist`, which needs no permission on the file and does not follow a link at the last
/// component. Case, case-fold and normalization aliases, firmlink spellings and volfs paths
/// all answer with the one stored spelling.
pub(crate) fn stored(path: &AbsPath) -> Seen<AbsPath> {
    let Ok(raw) = CString::new(path.as_path().as_os_str().as_bytes()) else {
        return Seen::Unknown(Unseen::Failed);
    };
    // SAFETY: a zeroed `attrlist` is valid; the two fields set below are the ones the call reads.
    let mut wanted: libc::attrlist = unsafe { std::mem::zeroed() };
    wanted.bitmapcount = libc::ATTR_BIT_MAP_COUNT;
    wanted.commonattr = libc::ATTR_CMN_FULLPATH;
    let mut out = FullPath {
        length: 0,
        path: libc::attrreference_t {
            attr_dataoffset: 0,
            attr_length: 0,
        },
        bytes: [0; libc::PATH_MAX as usize],
    };
    // SAFETY: `raw` is NUL-terminated, `wanted` is initialized, and `out` is writable for the
    // size passed with it.
    let failed = unsafe {
        libc::getattrlist(
            raw.as_ptr(),
            (&raw mut wanted).cast(),
            (&raw mut out).cast(),
            size_of::<FullPath>(),
            libc::FSOPT_NOFOLLOW,
        )
    };
    if failed != 0 {
        return unseen(&io::Error::last_os_error());
    }
    // The reference counts from its own address, and its length includes the final NUL.
    let reference = out.path;
    let start = usize::try_from(reference.attr_dataoffset)
        .ok()
        .and_then(|offset| offset.checked_sub(size_of::<libc::attrreference_t>()));
    let len = (reference.attr_length as usize).checked_sub(1);
    let spelled = start
        .zip(len)
        .and_then(|(start, len)| out.bytes.get(start..start.checked_add(len)?));
    match spelled.map(|bytes| AbsPath::parse(OsStr::from_bytes(bytes))) {
        Some(Ok(stored)) => Seen::Found(stored),
        Some(Err(_)) | None => Seen::Unknown(Unseen::Failed),
    }
}
