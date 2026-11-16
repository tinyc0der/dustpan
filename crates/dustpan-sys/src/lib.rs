//! The OS boundary. Every `unsafe` block and OS-specific API (`cfg(target_os)`, libc, windows-sys) lives in this crate.
//! `dustpan-core` does no I/O. The binary uses portable `std` I/O and no `unsafe`.
mod os;
mod privilege;

pub use os::os_facts;
pub use privilege::is_root;
