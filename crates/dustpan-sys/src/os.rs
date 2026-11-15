use dustpan_core::facts::{InstallMethod, OsFacts};
use dustpan_core::paths::{self, Os};
use std::path::PathBuf;

/// Name, version, arch and install method of the running system. Starts no subprocess.
pub fn os_facts() -> OsFacts {
    let env = |key: &str| std::env::var(key).ok();
    let cargo_bin = paths::cargo_bin_dir(Os::current(), &env);
    let install = match exe_path() {
        Some(exe) => InstallMethod::classify(&exe, cargo_bin.as_deref()),
        None => InstallMethod::Manual,
    };
    let (name, version) = name_and_version();
    OsFacts {
        name,
        version,
        arch: arch(),
        install,
    }
}

fn exe_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    // Homebrew links bin/dustpan into the Cellar. Windows would add a \\?\ prefix.
    if cfg!(windows) {
        Some(exe)
    } else {
        Some(std::fs::canonicalize(&exe).unwrap_or(exe))
    }
}

fn arch() -> &'static str {
    // `uname -m` says arm64 on macOS, so match it.
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "arm64"
    } else {
        std::env::consts::ARCH
    }
}

#[cfg(target_os = "macos")]
fn name_and_version() -> (String, String) {
    let mut buf = [0u8; 32];
    let mut len = buf.len();
    // SAFETY: the name is NUL-terminated and `buf` has room for `len` bytes.
    let rc = unsafe {
        libc::sysctlbyname(
            c"kern.osproductversion".as_ptr(),
            buf.as_mut_ptr().cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    let version = if rc == 0 {
        let end = buf[..len].iter().position(|&b| b == 0).unwrap_or(len);
        String::from_utf8_lossy(&buf[..end]).into_owned()
    } else {
        String::new()
    };
    ("macOS".to_owned(), version)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn name_and_version() -> (String, String) {
    let text = std::fs::read_to_string("/etc/os-release")
        .or_else(|_| std::fs::read_to_string("/usr/lib/os-release"))
        .unwrap_or_default();
    dustpan_core::facts::parse_os_release(&text)
}

#[cfg(windows)]
fn name_and_version() -> (String, String) {
    use windows_sys::Wdk::System::SystemServices::RtlGetVersion;
    use windows_sys::Win32::System::SystemInformation::OSVERSIONINFOW;

    // SAFETY: a zeroed OSVERSIONINFOW is valid once its size field is set, as RtlGetVersion requires.
    let info = unsafe {
        let mut info: OSVERSIONINFOW = std::mem::zeroed();
        info.dwOSVersionInfoSize = size_of::<OSVERSIONINFOW>() as u32;
        (RtlGetVersion(&mut info) == 0).then_some(info)
    };
    match info {
        Some(i) => (
            "Windows".to_owned(),
            format!(
                "{}.{}.{}",
                i.dwMajorVersion, i.dwMinorVersion, i.dwBuildNumber
            ),
        ),
        None => ("Windows".to_owned(), String::new()),
    }
}
