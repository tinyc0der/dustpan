//! The files every process holds open, read from the links in `/proc/<pid>/fd`, and the paths
//! Unix sockets are bound to, read from `/proc/net/unix`.
use crate::probe::{bound_socket, unseen};
use dustpan_core::probe::{OpenFile, OpenFiles, Pid, Seen, Unlisted};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs::ReadDir;
use std::io::{Error, ErrorKind};
use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::fs::MetadataExt as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn exited(e: &Error) -> bool {
    e.kind() == ErrorKind::NotFound || e.raw_os_error() == Some(libc::ESRCH)
}

enum Listed {
    Fds(ReadDir),
    Gone,
    Refused { mine: bool },
}

/// Opens the descriptor folder of the process at `process`, a folder of `/proc`.
///
/// The kernel shows a process's descriptors to the user it runs as and to root, and the
/// descriptor folder belongs to the user who may look. That is root for a process that turned
/// off inspection of itself, as `ssh-agent` does, whoever it runs as. `/proc/<pid>` itself
/// always belongs to the user the process runs as, so it cannot tell the two apart.
///
/// A process that is starting a new program is shown to root alone for a moment. A refusal
/// from a folder this user owns is therefore asked again, a millisecond apart.
fn list(process: &Path, me: libc::uid_t) -> Listed {
    let fd = process.join("fd");
    for _ in 0..4 {
        match std::fs::read_dir(&fd) {
            Ok(fds) => return Listed::Fds(fds),
            Err(e) if exited(&e) => return Listed::Gone,
            Err(_) => {}
        }
        match std::fs::symlink_metadata(&fd) {
            Ok(owner) if me != 0 && owner.uid() != me => return Listed::Refused { mine: false },
            Err(e) if exited(&e) => return Listed::Gone,
            Ok(_) | Err(_) => std::thread::sleep(Duration::from_millis(1)),
        }
    }
    Listed::Refused { mine: true }
}

pub(crate) fn open_files() -> Seen<OpenFiles> {
    let entries = match std::fs::read_dir("/proc") {
        Ok(entries) => entries,
        Err(e) => return unseen(&e),
    };
    let sockets = match std::fs::read("/proc/net/unix") {
        Ok(table) => bound_sockets(&table),
        Err(e) => return unseen(&e),
    };
    // SAFETY: `geteuid` has no preconditions.
    let me = unsafe { libc::geteuid() };
    let this = std::process::id();
    let mut open = OpenFiles::default();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid) = name.to_str().and_then(|n| n.parse::<u32>().ok()) else {
            continue;
        };
        // This process is left out: see `OpenFiles`.
        if pid == this {
            continue;
        }
        let fds = match list(&entry.path(), me) {
            Listed::Fds(fds) => fds,
            Listed::Gone => continue,
            Listed::Refused { mine } => {
                open.unlisted.push(Unlisted {
                    pid: Pid(pid),
                    mine,
                });
                continue;
            }
        };
        for fd in fds.flatten() {
            let Ok(target) = std::fs::read_link(fd.path()) else {
                continue;
            };
            let path = if target.is_absolute() {
                Some(target)
            } else {
                socket_inode(&target).and_then(|inode| sockets.get(&inode).cloned())
            };
            if let Some(path) = path {
                open.files.push(OpenFile {
                    pid: Pid(pid),
                    path,
                });
            }
        }
    }
    Seen::Found(open)
}

/// The inode in the name a socket's descriptor links to, `socket:[1234]`.
fn socket_inode(link: &Path) -> Option<u64> {
    let name = link.to_str()?.strip_prefix("socket:[")?.strip_suffix(']')?;
    name.parse().ok()
}

/// The path each Unix socket is bound to, by the socket's inode, from the text of
/// `/proc/net/unix`. A descriptor is a socket and no file, yet removing the path cuts the
/// process off from its clients. A row is `Num RefCount Protocol Flags Type St Inode Path`,
/// and the path is absent for an unbound socket and starts with `@` for an abstract one.
fn bound_sockets(table: &[u8]) -> HashMap<u64, PathBuf> {
    let row = |line: &[u8]| {
        let mut rest = line;
        let mut inode = None;
        for _ in 0..7 {
            rest = rest.trim_ascii_start();
            let end = rest.iter().position(|&b| b == b' ')?;
            inode = std::str::from_utf8(&rest[..end]).ok();
            rest = &rest[end + 1..];
        }
        let path = PathBuf::from(OsStr::from_bytes(rest));
        Some((inode?.parse().ok()?, bound_socket(path)?))
    };
    table
        .split(|&b| b == b'\n')
        .skip(1)
        .filter_map(row)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_socket_bound_to_an_absolute_path_is_listed() {
        let table = b"Num       RefCount Protocol Flags    Type St Inode Path
0000000000000000: 00000002 00000000 00010000 0001 01 17231 /run/user/1000/bus
0000000000000000: 00000002 00000000 00010000 0001 01    42 /tmp/with space/x.sock
0000000000000000: 00000002 00000000 00010000 0001 01 20418 @/tmp/.X11-unix/X0
0000000000000000: 00000003 00000000 00000000 0001 03 19012
0000000000000000: 00000002 00000000 00010000 0001 01 20500 relative.sock
";
        let sockets = bound_sockets(table);
        let mut inodes: Vec<_> = sockets.keys().copied().collect();
        inodes.sort_unstable();
        assert_eq!(inodes, [42, 17231]);
        assert_eq!(sockets[&42], Path::new("/tmp/with space/x.sock"));
        assert_eq!(socket_inode(Path::new("socket:[17231]")), Some(17231));
        assert_eq!(socket_inode(Path::new("pipe:[17231]")), None);
    }
}
