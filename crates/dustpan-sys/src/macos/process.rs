use super::bundle::Bundles;
use super::signature::{self, Signature, Unanswered};
use dustpan_core::probe::{App, Pid, Proc, ProcessTable, Seen, Unseen};
use std::ffi::OsString;
use std::io;
use std::os::unix::ffi::OsStringExt as _;
use std::path::Path;

/// A listed process the probe could not name: its pid, and the errno of the last attempt.
type Hidden = (libc::pid_t, Option<i32>);

/// One snapshot of every process's executable path, the bundles it sits in and its code
/// signature, through `proc_listallpids`, `proc_pidpath`, each bundle's `Info.plist` and
/// `csops`. No subprocess runs, so no deadline or `PATH` lookup is involved.
///
/// The bundle id is what ties a process to its cache folder: the path of
/// `/Applications/Ghostty.app/Contents/MacOS/ghostty` does not spell `com.mitchellh.ghostty`.
/// The entitlements are what tie it to a group container, which several apps share.
/// Arguments need `KERN_PROCARGS2`, which answers for the caller's own processes alone; M3 adds
/// them.
pub(crate) fn table() -> Seen<ProcessTable> {
    match snapshot() {
        Some((procs, hidden)) => Seen::Found(ProcessTable {
            complete: hidden.is_empty(),
            procs,
        }),
        None => Seen::Unknown(Unseen::Failed),
    }
}

/// Every pid the kernel lists. `None` when it gives no list.
pub(super) fn pids() -> Option<Vec<libc::pid_t>> {
    // SAFETY: a null buffer of size 0 asks only for the number of processes.
    let count = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
    let count = usize::try_from(count).ok()?;
    // Processes can start between the two calls.
    let mut pids: Vec<libc::pid_t> = vec![0; count + 64];
    let bytes = size_of_val(pids.as_slice()) as libc::c_int;
    // SAFETY: `pids` is writable for `bytes` bytes.
    let filled = unsafe { libc::proc_listallpids(pids.as_mut_ptr().cast(), bytes) };
    pids.truncate(usize::try_from(filled).ok().filter(|&n| n > 0)?);
    Some(pids)
}

fn snapshot() -> Option<(Vec<Proc>, Vec<Hidden>)> {
    let pids = pids()?;
    // SAFETY: `geteuid` has no preconditions.
    let me = unsafe { libc::geteuid() };
    // Root reads every process, so none is another user's to set aside.
    let mine = |uid: libc::uid_t| me == 0 || uid == me;
    // A process that cannot be told apart from any app. `None` when it exited meanwhile.
    let unidentified = |pid| match short_record(pid) {
        Short::Found { uid, .. } => Some(App::Unknown { mine: mine(uid) }),
        Short::Gone => None,
        Short::Unreadable(_) => Some(App::Unknown { mine: true }),
    };
    let mut bundles = Bundles::default();
    let mut scratch = Vec::new();
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
        let path_errno = io::Error::last_os_error().raw_os_error();
        let path = usize::try_from(len).ok().filter(|&len| len > 0);
        let (exe, mut app) = match path {
            Some(len) => {
                let exe = OsString::from_vec(buf[..len].to_vec());
                match bundles.enclosing(Path::new(&exe)) {
                    Ok(ids) => (exe, App::Bundles(ids)),
                    Err(_) => match unidentified(pid) {
                        Some(app) => (exe, app),
                        None => continue,
                    },
                }
            }
            None if path_errno == Some(libc::ESRCH) => continue,
            // The kernel cannot rebuild the path of a deleted or replaced binary, but the
            // process still runs. Its short record is readable for every user's processes and
            // gives a name cut to 16 bytes: enough to list it, not to say which app it is.
            None => match short_record(pid) {
                Short::Found { name, uid } => (name, App::Unknown { mine: mine(uid) }),
                // A process that exited since the listing, or a zombie, holds nothing open.
                Short::Gone => continue,
                Short::Unreadable(errno) => {
                    hidden.push((pid, errno.or(path_errno)));
                    continue;
                }
            },
        };
        let signature = match signature::of(pid, &mut scratch) {
            Ok(signature) => signature,
            Err(Unanswered::Gone) => continue,
            // The process may hold any group, so it may be any app.
            Err(Unanswered::Unreadable) => match unidentified(pid) {
                Some(unknown) => {
                    app = unknown;
                    Signature::default()
                }
                None => continue,
            },
        };
        procs.push(Proc {
            pid: Pid(id),
            exe,
            args: OsString::new(),
            app,
            signing_id: signature.id,
            app_groups: signature.app_groups,
        });
    }
    Some((procs, hidden))
}

pub(super) enum Short {
    Found { name: OsString, uid: libc::uid_t },
    Gone,
    Unreadable(Option<i32>),
}

pub(super) fn short_record(pid: libc::pid_t) -> Short {
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
            Short::Gone
        } else {
            Short::Unreadable(errno)
        };
    }
    if info.pbsi_status == libc::SZOMB {
        return Short::Gone;
    }
    let name = info.pbsi_comm.iter().take_while(|&&c| c != 0);
    Short::Found {
        name: OsString::from_vec(name.map(|&c| c as u8).collect()),
        uid: info.pbsi_uid,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_names_every_process_and_holds_this_one() {
        let (procs, hidden) = snapshot().expect("the process list is readable");
        assert!(hidden.is_empty(), "processes with no name: {hidden:?}");
        let me = std::process::id();
        let this = procs.iter().find(|p| p.pid == Pid(me)).unwrap();
        let exe = std::env::current_exe().unwrap().canonicalize().unwrap();
        assert_eq!(this.exe.as_os_str(), exe.as_os_str());
        assert_eq!(this.app, App::bare());
    }
}
