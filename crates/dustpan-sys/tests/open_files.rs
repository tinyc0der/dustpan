//! The snapshot of open files names what another process holds: a file, a folder and the path
//! of a bound Unix socket. It leaves this process out, and lets go when the holder exits
//! (SAFE-09).
#![cfg(unix)]

use dustpan_core::probe::{OpenFiles, Pid, Probe, Seen};
use dustpan_sys::LiveProbe;
use std::io::{BufRead as _, BufReader};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

const HOLD: &str = "DUSTPAN_TEST_HOLD";
const OPAQUE: &str = "DUSTPAN_TEST_OPAQUE";

/// What this binary runs as a child of the tests below: it holds a file, a folder and a bound
/// socket open in the folder it is given, says `held`, and waits to be killed. When asked, it
/// first turns off inspection of itself, as `ssh-agent` does.
#[test]
fn helper_holds_a_file_a_folder_and_a_socket_when_asked() {
    let Some(dir) = std::env::var_os(HOLD) else {
        return;
    };
    let dir = PathBuf::from(dir);
    let _file = std::fs::File::open(dir.join("held.bin")).unwrap();
    let _folder = std::fs::File::open(dir.join("held.d")).unwrap();
    let _socket = UnixListener::bind(dir.join("held.sock")).unwrap();
    #[cfg(target_os = "linux")]
    if std::env::var_os(OPAQUE).is_some() {
        // SAFETY: `PR_SET_DUMPABLE` takes one integer and touches no memory.
        assert_eq!(unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 0) }, 0);
    }
    println!("held");
    std::thread::sleep(std::time::Duration::from_secs(120));
}

/// A folder with a file and a folder in it, and a child that holds them open.
struct Held {
    _dir: tempfile::TempDir,
    /// The folder's physical path. The child is handed the path through the link `/var` on
    /// macOS, as `$TMPDIR` spells it.
    physical: PathBuf,
    holder: Child,
}

impl Held {
    fn by_a_child(opaque: bool) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let physical = dir.path().canonicalize().unwrap();
        std::fs::write(physical.join("held.bin"), "x").unwrap();
        std::fs::create_dir(physical.join("held.d")).unwrap();
        let mut holder = Command::new(std::env::current_exe().unwrap());
        holder
            .args([
                "--exact",
                "helper_holds_a_file_a_folder_and_a_socket_when_asked",
            ])
            .arg("--nocapture")
            .env(HOLD, dir.path())
            .stdout(Stdio::piped());
        if opaque {
            holder.env(OPAQUE, "1");
        }
        let mut holder = holder.spawn().unwrap();
        let mut said = BufReader::new(holder.stdout.take().unwrap()).lines();
        assert!(
            said.any(|line| line.unwrap() == "held"),
            "the helper never said held"
        );
        Self {
            _dir: dir,
            physical,
            holder,
        }
    }

    fn pid(&self) -> Pid {
        Pid(self.holder.id())
    }

    fn exit(&mut self) {
        self.holder.kill().unwrap();
        self.holder.wait().unwrap();
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        let _ = self.holder.kill();
        let _ = self.holder.wait();
    }
}

fn snapshot() -> OpenFiles {
    match LiveProbe::new().open_files() {
        Seen::Found(open) => open,
        other => panic!("no snapshot of open files: {other:?}"),
    }
}

fn holders(open: &OpenFiles, path: &Path) -> Vec<Pid> {
    let held = open.files.iter().filter(|file| file.path == path);
    held.map(|file| file.pid).collect()
}

#[test]
fn what_another_process_holds_open_is_listed_until_it_exits() {
    let mut held = Held::by_a_child(false);
    let _mine = std::fs::File::open(held.physical.join("held.bin")).unwrap();
    let while_held = snapshot();
    held.exit();
    let after_exit = snapshot();
    for name in ["held.bin", "held.d", "held.sock"] {
        let path = held.physical.join(name);
        // This test holds `held.bin` too, and is not listed.
        assert_eq!(holders(&while_held, &path), [held.pid()], "{name}");
        assert_eq!(holders(&after_exit, &path), [], "{name}");
    }
}

/// Linux shows the descriptors of such a process to root alone, so a run that is not root
/// sets it aside like another user's process. If it counted as this user's, one `ssh-agent`
/// would keep every path no rule protects.
#[cfg(target_os = "linux")]
#[test]
fn a_process_that_turned_off_inspection_is_set_aside() {
    use dustpan_core::probe::Unlisted;

    let held = Held::by_a_child(true);
    let open = snapshot();
    let pid = held.pid();
    let unlisted: Vec<_> = open.unlisted.iter().filter(|p| p.pid == pid).collect();
    // Root is shown everything, so there the process is listed like any other.
    // SAFETY: `geteuid` has no preconditions.
    if unsafe { libc::geteuid() } == 0 {
        assert_eq!(holders(&open, &held.physical.join("held.bin")), [pid]);
    } else {
        assert_eq!(unlisted, [&Unlisted { pid, mine: false }]);
    }
}

/// A process of this user that lists no files keeps every path no rule protects, so a runner
/// image that has one shows it here.
#[test]
fn every_process_of_this_user_lists_its_open_files() {
    let open = snapshot();
    assert!(!open.files.is_empty());
    let mine: Vec<_> = open
        .unlisted
        .iter()
        .filter(|process| process.mine)
        .collect();
    assert!(mine.is_empty(), "{mine:?}");
}
