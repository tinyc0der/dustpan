/// True when the effective user is root. Windows checks admin per task, so it is never "root" here.
pub fn is_root() -> bool {
    #[cfg(unix)]
    {
        // SAFETY: geteuid takes no arguments and cannot fail.
        unsafe { libc::geteuid() == 0 }
    }
    #[cfg(not(unix))]
    {
        false
    }
}
