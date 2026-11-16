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

pub fn whitelist(os: Os, env: Env, home: &AbsPath) -> Result<Whitelist, String> {
    let file = config::whitelist_path(os, env).and_then(|p| AbsPath::parse(p.as_os_str()).ok());
    let Some(file) = file else {
        return Err("cannot tell where the whitelist is: no absolute config folder is set".into());
    };
    whitelist::load(os, home, &file, read(file.as_path())).map_err(|e| e.to_string())
}
