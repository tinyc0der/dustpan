//! The OS boundary. Every `unsafe` block and OS-specific API (`cfg(target_os)`, libc, windows-sys) lives in this crate.
//! `dustpan-core` does no I/O. The binary uses portable `std` I/O and no `unsafe`.
mod config;
mod os;
mod privilege;
mod probe;
mod terminal;
mod volume;

#[cfg(target_os = "linux")]
mod linux {
    pub(crate) mod open;
    pub(crate) mod process;
}
#[cfg(target_os = "macos")]
mod macos {
    pub(crate) mod bundle;
    pub(crate) mod open;
    pub(crate) mod plist;
    pub(crate) mod process;
    pub(crate) mod signature;
    pub(crate) mod spelling;
}
#[cfg(unix)]
mod unix {
    pub(crate) mod identity;
}
#[cfg(windows)]
mod windows {
    pub(crate) mod identity;
    pub(crate) mod process;
}

pub use config::read_config;
pub use os::os_facts;
pub use privilege::is_root;
pub use probe::LiveProbe;
pub use terminal::{console_is_utf8, on_termination};
pub use volume::{BootVolume, boot_volume};
