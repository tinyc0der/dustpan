use dustpan_core::probe::{Pid, Proc, ProcessTable, Seen, Unseen};
use std::ffi::OsString;
use std::io;
use std::os::unix::ffi::OsStringExt as _;

/// A listed process the probe could not name: its pid, and the errno of the last attempt.
type Hidden = (libc::pid_t, Option<i32>);

/// One snapshot of every process's executable path, through `proc_listallpids` and
/// `proc_pidpath`. No subprocess runs, so no deadline or `PATH` lookup is involved.
///
/// The table holds executable paths only. Arguments need `KERN_PROCARGS2`, which answers for the
/// caller's own processes alone; M3 adds them with the open-file probe.
pub(crate) fn table() -> Seen<ProcessTable> {
    match snapshot() {
        Some((procs, hidden)) => Seen::Found(ProcessTable {
            complete: hidden.is_empty(),
            procs,
        }),
        None => Seen::Unknown(Unseen::Failed),
    }
}

fn snapshot() -> Option<(Vec<Proc>, Vec<Hidden>)> {
    // SAFETY: a null buffer of size 0 asks only for the number of processes.
    let count = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
    let count = usize::try_from(count).ok()?;
    // Processes can start between the two calls.
    let mut pids: Vec<libc::pid_t> = vec![0; count + 64];
    let bytes = size_of_val(pids.as_slice()) as libc::c_int;
    // SAFETY: `pids` is writable for `bytes` bytes.
    let filled = unsafe { libc::proc_listallpids(pids.as_mut_ptr().cast(), bytes) };
    pids.truncate(usize::try_from(filled).ok().filter(|&n| n > 0)?);

    let mut procs = Vec::with_capacity(pids.len());
    let mut hidden = Vec::new();
    let mut buf = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    for pid in pids {
        // Pid 0 is the kernel, which has no executable.
        let Some(id) = u32::try_from(pid).ok().filter(|&id| id != 0) else {
            continue;
        };
        // SAFETY: `buf` is writable for its whole length.
        let len = unsafe { libc::proc_pidpath(pid, buf.as_mut_ptr().cast(), buf.len() as u32) };
        let exe = match usize::try_from(len) {
            Ok(len) if len > 0 => Some(OsString::from_vec(buf[..len].to_vec())),
            _ => match short_name(pid) {
                Named::Yes(name) => Some(name),
                // A process that exited since the listing, or a zombie, holds nothing open.
                Named::Gone => None,
                Named::No(errno) => {
                    hidden.push((pid, errno));
                    None
                }
            },
        };
        if let Some(exe) = exe {
            procs.push(Proc {
                pid: Pid(id),
                exe,
                args: OsString::new(),
            });
        }
    }
    Some((procs, hidden))
}

enum Named {
    Yes(OsString),
    Gone,
    No(Option<i32>),
}

/// The kernel's short name for a process whose executable path it will not give: the path of a
/// deleted or replaced binary cannot be rebuilt, but the process still runs. The short record
/// is readable for every user's processes.
fn short_name(pid: libc::pid_t) -> Named {
    let path_errno = io::Error::last_os_error().raw_os_error();
    if path_errno == Some(libc::ESRCH) {
        return Named::Gone;
    }
    // SAFETY: a zeroed `proc_bsdshortinfo` is a valid value to overwrite.
    let mut info: libc::proc_bsdshortinfo = unsafe { std::mem::zeroed() };
    let size = size_of::<libc::proc_bsdshortinfo>() as libc::c_int;
    // SAFETY: `info` is writable for `size` bytes.
    let got = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDT_SHORTBSDINFO,
            0,
            (&raw mut info).cast(),
            size,
        )
    };
    if got != size {
        let errno = io::Error::last_os_error().raw_os_error();
        return if errno == Some(libc::ESRCH) {
            Named::Gone
        } else {
            Named::No(errno.or(path_errno))
        };
    }
    if info.pbsi_status == libc::SZOMB {
        return Named::Gone;
    }
    let name = info.pbsi_comm.iter().take_while(|&&c| c != 0);
    Named::Yes(OsString::from_vec(name.map(|&c| c as u8).collect()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_names_every_process_and_holds_this_one() {
        let (procs, hidden) = snapshot().expect("the process list is readable");
        assert!(hidden.is_empty(), "processes with no name: {hidden:?}");
        let me = std::process::id();
        let this = procs.iter().find(|p| p.pid == Pid(me));
        let exe = std::env::current_exe().unwrap().canonicalize().unwrap();
        assert_eq!(this.map(|p| p.exe.as_os_str()), Some(exe.as_os_str()));
    }
}
