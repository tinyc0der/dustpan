//! Pure domain logic. Nothing here calls the OS, so every rule is testable with plain values.
//!
//! The guard reaches the file system only through [`probe::Probe`], which `dustpan-sys`
//! implements with syscalls and `probe::fake` implements in memory.
#![forbid(unsafe_code)]

pub mod candidate;
pub mod facts;
pub mod guard;
pub mod identity;
pub mod path;
pub mod paths;
pub mod probe;
mod protect;
pub mod size;
pub mod time;
