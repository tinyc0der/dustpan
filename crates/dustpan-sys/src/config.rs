//! Reading a config file the way Mole's `mole_whitelist_file_state` judges one: it is missing
//! only when a folder or file on its path does not exist. A broken link, a folder that is not
//! one, or anything but a regular file stops the read, so it never authorizes the defaults.
use dustpan_core::config::Read;
use dustpan_core::path::{AbsPath, escaped};
use std::fs::{File, Metadata};
use std::io::{ErrorKind, Read as _};
use std::path::Path;

/// The most a config file may hold. A hand-kept whitelist of 10,000 entries is under half of
/// it, and a link to a device or a log that keeps growing stops here instead of filling memory.
const CONFIG_CAP: u64 = 1024 * 1024;

pub fn read_config(file: &AbsPath) -> Read {
    match inspect(file.as_path()) {
        Ok(false) => Read::Missing,
        Ok(true) => match read_regular(file.as_path()) {
            Ok(text) => Read::Text(text),
            Err(why) => Read::Unreadable { why },
        },
        Err(why) => Read::Unreadable { why },
    }
}

/// Walks `file` from the top, one `lstat` per component, following links. `Ok(false)` when a
/// component does not exist, `Ok(true)` when every folder is one and `file` is a regular file.
fn inspect(file: &Path) -> Result<bool, String> {
    let mut lineage: Vec<&Path> = file.ancestors().collect();
    lineage.pop();
    lineage.reverse();
    let last = lineage.len().saturating_sub(1);
    for (depth, at) in lineage.into_iter().enumerate() {
        let shown = escaped(at.as_os_str());
        let meta = match std::fs::symlink_metadata(at) {
            Ok(meta) => meta,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(format!("cannot inspect {shown}: {e}")),
        };
        let meta = if meta.file_type().is_symlink() {
            followed(at)?
        } else {
            meta
        };
        if depth < last && !meta.is_dir() {
            return Err(format!("{shown} is not a folder"));
        }
        if depth == last && !meta.is_file() {
            return Err(format!("{shown} is not a regular file"));
        }
    }
    Ok(true)
}

fn followed(link: &Path) -> Result<Metadata, String> {
    let shown = escaped(link.as_os_str());
    match std::fs::metadata(link) {
        Ok(meta) => Ok(meta),
        Err(e) if e.kind() == ErrorKind::NotFound => {
            let target = std::fs::read_link(link)
                .map(|t| escaped(t.as_os_str()).to_string())
                .unwrap_or_else(|_| "a path".to_owned());
            Err(format!(
                "{shown} is a link to {target}, which does not exist"
            ))
        }
        Err(e) => Err(format!("cannot follow the link {shown}: {e}")),
    }
}

/// Opens without blocking, so a FIFO swapped in after the walk cannot hold the run, and reads
/// at most one byte past the cap from what `fstat` says is a regular file.
fn read_regular(file: &Path) -> Result<String, String> {
    let shown = escaped(file.as_os_str());
    let opened = open(file).map_err(|e| format!("cannot open it: {e}"))?;
    let meta = opened
        .metadata()
        .map_err(|e| format!("cannot inspect it: {e}"))?;
    if !meta.is_file() {
        return Err(format!("{shown} is not a regular file"));
    }
    let too_big = || format!("it is larger than the {CONFIG_CAP} bytes a config file may hold");
    if meta.len() > CONFIG_CAP {
        return Err(too_big());
    }
    let mut bytes = Vec::new();
    opened
        .take(CONFIG_CAP + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("cannot read it: {e}"))?;
    if bytes.len() as u64 > CONFIG_CAP {
        return Err(too_big());
    }
    String::from_utf8(bytes).map_err(|_| "it is not UTF-8 text".to_owned())
}

#[cfg(unix)]
fn open(file: &Path) -> std::io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt as _;
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(file)
}

#[cfg(not(unix))]
fn open(file: &Path) -> std::io::Result<File> {
    File::open(file)
}
