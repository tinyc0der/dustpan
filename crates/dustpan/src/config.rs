//! Reads config files for core, which does no I/O. A file that is not there and a file that
//! cannot be read are different answers, and only the first lets a run go on.
use dustpan_core::config::{self, Read};
use dustpan_core::path::AbsPath;
use dustpan_core::paths::{Env, Os};
use dustpan_core::whitelist::{self, Whitelist};
use std::io::ErrorKind;
use std::path::Path;

fn read(path: &Path) -> Read {
    match std::fs::read_to_string(path) {
        Ok(text) => Read::Text(text),
        Err(e) if e.kind() == ErrorKind::NotFound => Read::Missing,
        Err(e) => Read::Unreadable { why: e.to_string() },
    }
}

/// The whitelist for `home`. `Err` is the sentence to print before the command stops: a
/// whitelist that cannot be read is not an empty one, so no path may be judged without it
/// (SAFE-11).
pub fn whitelist(os: Os, env: Env, home: &AbsPath) -> Result<Whitelist, String> {
    let file = config::whitelist_path(os, env).and_then(|p| AbsPath::parse(p.as_os_str()).ok());
    let Some(file) = file else {
        return Err("cannot tell where the whitelist is: no absolute config folder is set".into());
    };
    whitelist::load(os, home, &file, read(file.as_path())).map_err(|e| e.to_string())
}
