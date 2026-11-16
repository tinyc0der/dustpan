//! The operations log, read for `dustpan history` and the History screen. Read-only (SAFE-37):
//! it opens `operations.log.old` and `operations.log` for reading and nothing else, so it never
//! creates, rotates or changes a log file, and it never makes the log folder.
use dustpan_core::history::History;
use dustpan_core::path::escaped;
use dustpan_core::paths::{self, Env, Os};
use dustpan_sys::FileStore;
use std::io;
use std::path::Path;

#[derive(Clone, Debug)]
pub struct Loaded {
    pub history: History,
    /// The current log's path, as `dustpan history` names it.
    pub log: String,
}

/// The file's bytes, empty when it does not exist, or the message that names it.
fn read(dir: &Path, name: &str) -> Result<Vec<u8>, String> {
    let path = dir.join(name);
    match FileStore::read(&path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        read => read.map_err(|e| format!("cannot read {}: {e}", escaped(path.as_os_str()))),
    }
}

/// Both log files as a [`History`]. An absent log is an empty history. The error is the sentence
/// that follows `dustpan: ` in the CLI.
pub fn load(env: Env) -> Result<Loaded, String> {
    let Some(dir) = paths::log_dir(Os::current(), env) else {
        return Err(
            "the home folder is not set or not an absolute path, so there is no log folder to read"
                .to_owned(),
        );
    };
    let old = read(&dir, FileStore::OLD)?;
    let current = read(&dir, FileStore::FILE)?;
    Ok(Loaded {
        history: History::read(&old, &current),
        log: dir.join(FileStore::FILE).to_string_lossy().into_owned(),
    })
}
