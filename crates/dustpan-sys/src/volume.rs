#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BootVolume {
    pub name: String,
    pub free_bytes: u64,
}

#[cfg(unix)]
pub fn boot_volume() -> Option<BootVolume> {
    use std::ffi::CString;

    let root = CString::new("/").ok()?;
    let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: `root` is a valid NUL-terminated path and `stat` is a writable statvfs.
    let status = unsafe { libc::statvfs(root.as_ptr(), stat.as_mut_ptr()) };
    if status != 0 {
        return None;
    }
    // SAFETY: statvfs returned 0, so it filled the struct.
    let stat = unsafe { stat.assume_init() };
    #[allow(clippy::unnecessary_cast)]
    let free_bytes = (stat.f_bavail as u64).saturating_mul(stat.f_frsize as u64);
    Some(BootVolume {
        name: root_volume_name(),
        free_bytes,
    })
}

/// macOS lists the boot volume in /Volumes as a symlink to "/". Elsewhere the root has no name.
#[cfg(unix)]
fn root_volume_name() -> String {
    let named = std::fs::read_dir("/Volumes").ok().and_then(|dir| {
        dir.flatten()
            .find(|entry| {
                std::fs::read_link(entry.path()).is_ok_and(|to| to == std::path::Path::new("/"))
            })
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
    });
    named.unwrap_or_else(|| "/".to_string())
}

#[cfg(windows)]
pub fn boot_volume() -> Option<BootVolume> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".to_string());
    let root: Vec<u16> = std::ffi::OsString::from(format!("{drive}\\"))
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut free_to_caller = 0u64;
    // SAFETY: `root` is NUL-terminated and the out pointer is valid; the two unused outs may be null.
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            root.as_ptr(),
            &mut free_to_caller,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    (ok != 0).then_some(BootVolume {
        name: drive,
        free_bytes: free_to_caller,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boot_volume_reports_free_space_and_a_name() {
        let volume = boot_volume().expect("the boot volume answers");
        assert!(volume.free_bytes > 0);
        assert!(!volume.name.is_empty());
    }
}
