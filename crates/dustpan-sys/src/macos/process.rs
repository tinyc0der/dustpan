use dustpan_core::probe::{Pid, Proc, ProcessTable, Seen, Unseen};
use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt as _;

/// One snapshot of every process's executable path, through `proc_listallpids` and
/// `proc_pidpath`. No subprocess runs, so no deadline or `PATH` lookup is involved.
///
/// The table holds executable paths only. Arguments need `KERN_PROCARGS2`, which answers for the
/// caller's own processes alone; M3 adds them with the open-file probe.
pub(crate) fn table() -> Seen<ProcessTable> {
    // SAFETY: a null buffer of size 0 asks only for the number of processes.
    let count = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
    let Ok(count) = usize::try_from(count) else {
        return Seen::Unknown(Unseen::Failed);
    };
    // Processes can start between the two calls.
    let mut pids: Vec<libc::pid_t> = vec![0; count + 64];
    let bytes = size_of_val(pids.as_slice()) as libc::c_int;
    // SAFETY: `pids` is writable for `bytes` bytes.
    let filled = unsafe { libc::proc_listallpids(pids.as_mut_ptr().cast(), bytes) };
    match usize::try_from(filled) {
        Ok(filled) if filled > 0 => pids.truncate(filled),
        _ => return Seen::Unknown(Unseen::Failed),
    }

    let mut complete = true;
    let mut procs = Vec::with_capacity(pids.len());
    let mut buf = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    for pid in pids {
        // Pid 0 is the kernel, which has no executable.
        let Some(id) = u32::try_from(pid).ok().filter(|&id| id != 0) else {
            continue;
        };
        // SAFETY: `buf` is writable for its whole length.
        let len = unsafe { libc::proc_pidpath(pid, buf.as_mut_ptr().cast(), buf.len() as u32) };
        match usize::try_from(len) {
            Ok(len) if len > 0 => procs.push(Proc {
                pid: Pid(id),
                exe: OsString::from_vec(buf[..len].to_vec()),
                args: OsString::new(),
            }),
            _ => {
                // A process that exited since the listing is gone, not hidden.
                let gone = std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH);
                complete &= gone;
            }
        }
    }
    Seen::Found(ProcessTable { complete, procs })
}
