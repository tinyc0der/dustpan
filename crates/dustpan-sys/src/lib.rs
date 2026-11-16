//! The OS boundary. Every `unsafe` block and OS-specific API (`cfg(target_os)`, libc, windows-sys) lives in this crate.
//! `dustpan-core` does no I/O. The binary uses portable `std` I/O and no `unsafe`.
mod clock;
mod config;
mod disk;
mod home;
mod oplog;
mod os;
mod privilege;
mod probe;
mod terminal;
mod volume;

#[cfg(target_os = "linux")]
mod linux {
    pub(crate) mod delete;
    pub(crate) mod open;
    pub(crate) mod process;
    pub(crate) mod trash;
}
#[cfg(target_os = "macos")]
mod macos {
    pub(crate) mod bundle;
    pub(crate) mod delete;
    pub(crate) mod open;
    pub(crate) mod plist;
    pub(crate) mod process;
    pub(crate) mod signature;
    pub(crate) mod spelling;
    pub(crate) mod trash;
}
#[cfg(unix)]
mod unix {
    pub(crate) mod delete;
    pub(crate) mod identity;
}
#[cfg(windows)]
mod windows {
    pub(crate) mod delete;
    pub(crate) mod identity;
    pub(crate) mod process;
    pub(crate) mod trash;
}

pub use clock::SystemClock;
pub use config::read_config;
pub use disk::{DirHandle, LiveDisk};
pub use home::account_home;
pub use oplog::FileStore;
pub use os::os_facts;
pub use privilege::is_root;
pub use probe::LiveProbe;
pub use terminal::{console_is_utf8, on_termination};
pub use volume::{BootVolume, boot_volume};
