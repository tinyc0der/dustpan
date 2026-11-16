//! The files every process holds open, read from the kernel through libproc: `proc_listallpids`,
//! `proc_pidinfo(PROC_PIDLISTFDS)` and `proc_pidfdinfo`. No subprocess runs and no file is read.
//!
//! The cost is one call per process and one per open file, folder or socket, up to
//! `MAX_DESCRIPTORS`. `docs/perf.md` has the measured cost.
use super::process::{self, Short};
use crate::probe::{MAX_DESCRIPTORS, bound_socket};
use dustpan_core::probe::{OpenFile, OpenFiles, Pid, Seen, Unlisted, Unseen};
use std::ffi::OsString;
use std::io;
use std::os::unix::ffi::OsStringExt as _;
use std::path::PathBuf;

// From `<sys/proc_info.h>`: the two `proc_pidfdinfo` questions asked here, and where their
// answers hold what is read. The `libc` crate binds none of them.

/// Answered with a `struct vnode_fdinfowithpath`, which ends in the path.
const PROC_PIDFDVNODEPATHINFO: libc::c_int = 2;
const VNODE_INFO: usize = 1200;
const VNODE_PATH: usize = VNODE_INFO - libc::MAXPATHLEN as usize;

/// Answered with a `struct socket_fdinfo`.
const PROC_PIDFDSOCKETINFO: libc::c_int = 3;
const SOCKET_INFO: usize = 792;
/// `psi.soi_family` and `psi.soi_kind`, both `int`.
const SOCKET_FAMILY: usize = 184;
const SOCKET_KIND: usize = 256;
const SOCKINFO_UN: i32 = 3;
/// `psi.soi_proto.pri_un.unsi_addr`, the address the socket is bound to: a `sockaddr_un` in
/// `SOCK_MAXADDRLEN` bytes, with its path two bytes in.
const BOUND_PATH: std::ops::Range<usize> = 282..535;

const ENTRY: usize = size_of::<libc::proc_fdinfo>();
/// `kern.maxfilesperproc` is 122,880 on the bench host. A longer list is not believed.
const MAX_ENTRIES: usize = 1 << 20;

enum Listed {
    Fds,
    Gone,
    Refused,
}

pub(crate) fn open_files() -> Seen<OpenFiles> {
    snapshot(MAX_DESCRIPTORS)
}

fn snapshot(most: usize) -> Seen<OpenFiles> {
    let Some(pids) = process::pids() else {
        return Seen::Unknown(Unseen::Failed);
    };
    // SAFETY: `geteuid` has no preconditions.
    let me = unsafe { libc::geteuid() };
    let this = std::process::id();
    let mut open = OpenFiles::default();
    let mut fds = Vec::new();
    let mut info = [0u8; VNODE_INFO];
    let mut seen = 0;
    for pid in pids {
        // Pid 0 is the kernel.
        let Some(id) = u32::try_from(pid).ok().filter(|&id| id != 0 && id != this) else {
            continue;
        };
        match list(pid, &mut fds) {
            Listed::Fds => {}
            Listed::Gone => continue,
            Listed::Refused => {
                let mine = match process::short_record(pid) {
                    Short::Found { uid, .. } => me == 0 || uid == me,
                    // A process that exited since the listing, or a zombie, holds nothing open.
                    Short::Gone => continue,
                    Short::Unreadable(_) => true,
                };
                open.unlisted.push(Unlisted { pid: Pid(id), mine });
                continue;
            }
        }
        seen += fds.len();
        if seen > most {
            return Seen::Unknown(Unseen::TimedOut);
        }
        for fd in &fds {
            let path = match fd.proc_fdtype as libc::c_int {
                libc::PROX_FDTYPE_VNODE => vnode_path(pid, fd.proc_fd, &mut info),
                libc::PROX_FDTYPE_SOCKET => socket_path(pid, fd.proc_fd, &mut info),
                _ => None,
            };
            if let Some(path) = path {
                open.files.push(OpenFile { pid: Pid(id), path });
            }
        }
    }
    Seen::Found(open)
}

/// Fills `fds` with the open descriptors of `pid`.
fn list(pid: libc::pid_t, fds: &mut Vec<libc::proc_fdinfo>) -> Listed {
    let empty = libc::proc_fdinfo {
        proc_fd: 0,
        proc_fdtype: 0,
    };
    fds.resize(fds.capacity().max(256), empty);
    loop {
        let room = (fds.len() * ENTRY) as libc::c_int;
        // SAFETY: `__error` returns this thread's `errno`, which is writable.
        unsafe { *libc::__error() = 0 };
        // SAFETY: `fds` is writable for `room` bytes.
        let got = unsafe {
            libc::proc_pidinfo(pid, libc::PROC_PIDLISTFDS, 0, fds.as_mut_ptr().cast(), room)
        };
        let Some(got) = usize::try_from(got).ok().filter(|&got| got > 0) else {
            // libproc returns 0 for a failure and for a process with nothing open.
            return match io::Error::last_os_error().raw_os_error() {
                Some(0) => {
                    fds.clear();
                    Listed::Fds
                }
                Some(libc::ESRCH) => Listed::Gone,
                _ => Listed::Refused,
            };
        };
        if got / ENTRY < fds.len() {
            fds.truncate(got / ENTRY);
            return Listed::Fds;
        }
        // The kernel fills what fits and does not say how much was left out.
        if fds.len() >= MAX_ENTRIES {
            return Listed::Refused;
        }
        fds.resize(fds.len() * 2, empty);
    }
}

/// Asks one `proc_pidfdinfo` question. False when the kernel gives no whole answer.
fn ask(pid: libc::pid_t, fd: i32, flavor: libc::c_int, answer: &mut [u8]) -> bool {
    let size = answer.len() as libc::c_int;
    // SAFETY: `answer` is writable for `size` bytes.
    let got = unsafe { libc::proc_pidfdinfo(pid, fd, flavor, answer.as_mut_ptr().cast(), size) };
    got == size
}

fn path(bytes: &[u8]) -> Option<PathBuf> {
    let len = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    (len > 0).then(|| PathBuf::from(OsString::from_vec(bytes[..len].to_vec())))
}

/// The path of the file or folder behind one descriptor. `None` when it has none to give:
/// the descriptor closed since the listing, the file was unlinked, or the kernel refuses
/// (`EPERM`). The refusal is a security policy's (`mac_vnode_check_fsgetpath`). On the bench
/// host it met 59 of 5,493 descriptors, all of eight Apple daemons, and those of `routined`
/// that could be traced sit in a data vault, which this process cannot open or remove either.
fn vnode_path(pid: libc::pid_t, fd: i32, info: &mut [u8; VNODE_INFO]) -> Option<PathBuf> {
    ask(pid, fd, PROC_PIDFDVNODEPATHINFO, info).then_some(())?;
    path(&info[VNODE_PATH..])
}

/// The path a Unix socket is bound to. The descriptor is a socket and no file, yet removing
/// the path cuts the process off from its clients: Docker's `engine.sock` sits in
/// `~/Library/Caches`.
fn socket_path(pid: libc::pid_t, fd: i32, info: &mut [u8; VNODE_INFO]) -> Option<PathBuf> {
    let info = &mut info[..SOCKET_INFO];
    ask(pid, fd, PROC_PIDFDSOCKETINFO, info).then_some(())?;
    let int = |at: usize| i32::from_ne_bytes([info[at], info[at + 1], info[at + 2], info[at + 3]]);
    if int(SOCKET_FAMILY) != libc::AF_UNIX || int(SOCKET_KIND) != SOCKINFO_UN {
        return None;
    }
    bound_socket(path(&info[BOUND_PATH])?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_snapshot_past_its_descriptor_limit_is_unknown() {
        assert_eq!(snapshot(0), Seen::Unknown(Unseen::TimedOut));
        assert!(matches!(snapshot(MAX_DESCRIPTORS), Seen::Found(_)));
    }
}
