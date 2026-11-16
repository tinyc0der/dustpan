//! The account's home folder as the user database records it. `HOME` and `USERPROFILE` are never
//! read here: a guard anchored at them alone follows whatever the environment says (I-64).
use std::io;
use std::path::PathBuf;

/// The home folder of the user this process runs as. `Ok(None)` when the database has no entry
/// for the user, or the entry names no folder.
pub fn account_home() -> io::Result<Option<PathBuf>> {
    imp::account_home()
}

#[cfg(unix)]
mod imp {
    use std::ffi::{CStr, OsStr};
    use std::io;
    use std::os::unix::ffi::OsStrExt as _;
    use std::path::PathBuf;

    const MAX_BUFFER: usize = 1 << 20;

    pub(super) fn account_home() -> io::Result<Option<PathBuf>> {
        // SAFETY: getuid takes no arguments and cannot fail.
        let uid = unsafe { libc::getuid() };
        let mut buf: Vec<libc::c_char> = vec![0; 1024];
        loop {
            let mut entry = std::mem::MaybeUninit::<libc::passwd>::uninit();
            let mut found: *mut libc::passwd = std::ptr::null_mut();
            // SAFETY: `entry` and `buf` are writable for the sizes passed, and the call sets
            // `found` to null or to `entry`.
            let err = unsafe {
                libc::getpwuid_r(
                    uid,
                    entry.as_mut_ptr(),
                    buf.as_mut_ptr(),
                    buf.len(),
                    &raw mut found,
                )
            };
            if err == libc::ERANGE && buf.len() < MAX_BUFFER {
                buf.resize(buf.len() * 2, 0);
                continue;
            }
            if found.is_null() {
                return match err {
                    // getpwuid_r(3): a missing user may be reported as any of these.
                    0 | libc::ENOENT | libc::ESRCH | libc::EBADF | libc::EPERM => Ok(None),
                    err => Err(io::Error::from_raw_os_error(err)),
                };
            }
            // SAFETY: `found` points at `entry`, which the call filled.
            let dir = unsafe { (*found).pw_dir };
            if dir.is_null() {
                return Ok(None);
            }
            // SAFETY: `pw_dir` is a NUL-terminated string inside `buf`, which is still alive.
            let dir = unsafe { CStr::from_ptr(dir) }.to_bytes();
            return Ok((!dir.is_empty()).then(|| PathBuf::from(OsStr::from_bytes(dir))));
        }
    }
}

#[cfg(windows)]
mod imp {
    use std::ffi::OsString;
    use std::io;
    use std::os::windows::ffi::OsStringExt as _;
    use std::path::PathBuf;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Security::TOKEN_QUERY;
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    use windows_sys::Win32::UI::Shell::GetUserProfileDirectoryW;

    /// The profile folder the registry records for the process token's user.
    pub(super) fn account_home() -> io::Result<Option<PathBuf>> {
        let mut token: HANDLE = std::ptr::null_mut();
        // SAFETY: the current-process pseudo-handle needs no closing, and `token` is writable.
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &raw mut token) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let profile = profile_folder(token);
        // SAFETY: `token` came from OpenProcessToken and is closed once, here.
        unsafe { CloseHandle(token) };
        profile.map(Some)
    }

    fn profile_folder(token: HANDLE) -> io::Result<PathBuf> {
        let mut len = 0u32;
        // SAFETY: a null buffer asks for the length the path needs, written to `len`.
        unsafe { GetUserProfileDirectoryW(token, std::ptr::null_mut(), &raw mut len) };
        let mut buf = vec![0u16; len as usize];
        // SAFETY: `buf` holds the `len` UTF-16 units the call above asked for.
        if unsafe { GetUserProfileDirectoryW(token, buf.as_mut_ptr(), &raw mut len) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let end = buf.iter().position(|&unit| unit == 0).unwrap_or(buf.len());
        Ok(PathBuf::from(OsString::from_wide(&buf[..end])))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_account_home_is_an_existing_absolute_folder() {
        let home = account_home()
            .expect("the user database answers")
            .expect("the test user has a home");
        assert!(home.is_absolute(), "{}", home.display());
        assert!(home.is_dir(), "{}", home.display());
    }
}
