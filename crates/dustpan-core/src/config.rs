//! What the binary hands core after it tries to read a config file. Core does no I/O, so the
//! three outcomes are data: a missing file means "use the built-in answer", an unreadable one
//! means "stop", and the two must never be confused (Mole's WL-01 read an unreadable whitelist
//! as an empty one and cleaned what it protected).
use crate::paths::{self, Env, Os};
use std::path::PathBuf;

pub const WHITELIST_FILE: &str = "whitelist";

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Read {
    /// The file does not exist (`ENOENT`).
    Missing,
    Text(String),
    /// The file may exist, but this process could not read it as UTF-8 text. `why` is the OS
    /// error, ready to print.
    Unreadable {
        why: String,
    },
}

pub fn whitelist_path(os: Os, env: Env) -> Option<PathBuf> {
    Some(paths::config_dir(os, env)?.join(WHITELIST_FILE))
}
