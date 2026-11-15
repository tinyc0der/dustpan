use crate::probe::unseen;
use dustpan_core::probe::{App, Pid, Proc, ProcessTable, Seen};
use std::ffi::OsString;
use std::io::ErrorKind;
use std::os::unix::ffi::OsStringExt as _;

/// One snapshot of every process's command line, from `/proc/<pid>/cmdline`.
pub(crate) fn table() -> Seen<ProcessTable> {
    let entries = match std::fs::read_dir("/proc") {
        Ok(entries) => entries,
        Err(e) => return unseen(&e),
    };
    let mut complete = true;
    let mut saw_init = false;
    let mut procs = Vec::new();
    for entry in entries.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|n| n.parse::<u32>().ok())
        else {
            continue;
        };
        saw_init |= pid == 1;
        match std::fs::read(entry.path().join("cmdline")) {
            Ok(line) => {
                let mut words = line
                    .split(|&b| b == 0)
                    .filter(|word| !word.is_empty())
                    .map(<[u8]>::to_vec);
                // A kernel thread or a zombie has an empty command line and owns no cache.
                if let Some(exe) = words.next() {
                    procs.push(Proc {
                        pid: Pid(pid),
                        exe: OsString::from_vec(exe),
                        args: OsString::from_vec(words.collect::<Vec<_>>().join(&b' ')),
                        // Linux has no bundles: the command line is what names an owner.
                        app: App::bare(),
                    });
                }
            }
            // The process exited since the listing: gone, not hidden.
            Err(e) if e.kind() == ErrorKind::NotFound || e.raw_os_error() == Some(libc::ESRCH) => {}
            Err(_) => complete = false,
        }
    }
    // `/proc` mounted with `hidepid` lists the caller's processes only, and init is not one.
    complete &= saw_init;
    Seen::Found(ProcessTable { complete, procs })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_complete_and_holds_this_process() {
        let Seen::Found(table) = table() else {
            panic!("/proc is not readable");
        };
        assert!(table.complete);
        let me = Pid(std::process::id());
        let this = table.procs.iter().find(|p| p.pid == me);
        let exe = std::env::args_os().next();
        assert_eq!(this.map(|p| p.exe.clone()), exe);
    }
}
