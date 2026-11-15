//! Pure domain logic. Nothing here calls the OS, so every rule is testable with plain values.
#![forbid(unsafe_code)]

pub mod facts;
pub mod paths;
pub mod time;
