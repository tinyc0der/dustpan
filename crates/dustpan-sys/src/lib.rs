//! The OS boundary. Every `unsafe` block and OS API in the workspace lives in this crate.
mod os;
mod privilege;

pub use os::os_facts;
pub use privilege::is_root;
