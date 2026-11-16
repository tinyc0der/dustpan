//! The snapshot of open files names what another process holds: a file, a folder and the path
//! of a bound Unix socket. It leaves this process out, and lets go when the holder exits
//! (SAFE-09).
#![cfg(unix)]

use dustpan_core::probe::{OpenFiles, Pid, Probe, Seen};
use dustpan_sys::LiveProbe;
use std::io::{BufRead as _, BufReader};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const HOLD: &str = "DUSTPAN_TEST_HOLD";

/// What this binary runs as a child of the test below: it holds a file, a folder and a bound
/// socket open in the folder it is given, says `held`, and waits to be killed.
#[test]
fn helper_holds_a_file_a_folder_and_a_socket_when_asked() {
    let Some(dir) = std::env::var_os(HOLD) else {
        return;
    };
    let dir = PathBuf::from(dir);
    let _file = std::fs::File::open(dir.join("held.bin")).unwrap();
    let _folder = std::fs::File::open(dir.join("held.d")).unwrap();
    let _socket = UnixListener::bind(dir.join("held.sock")).unwrap();
    println!("held");
    std::thread::sleep(std::time::Duration::from_secs(120));
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
    let dir = tempfile::tempdir().unwrap();
    // The helper is handed the path through the link `/var` on macOS, as `$TMPDIR` spells it.
    let physical = dir.path().canonicalize().unwrap();
    let names = ["held.bin", "held.d", "held.sock"];
    std::fs::write(physical.join("held.bin"), "x").unwrap();
    std::fs::create_dir(physical.join("held.d")).unwrap();
    let _mine = std::fs::File::open(physical.join("held.bin")).unwrap();

    let mut holder = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "helper_holds_a_file_a_folder_and_a_socket_when_asked",
        ])
        .arg("--nocapture")
        .env(HOLD, dir.path())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut said = BufReader::new(holder.stdout.take().unwrap()).lines();
    let held = said.any(|line| line.unwrap() == "held");
    let while_held = snapshot();
    holder.kill().unwrap();
    holder.wait().unwrap();
    let after_exit = snapshot();

    assert!(held, "the helper never said held");
    for name in names {
        let path = physical.join(name);
        // This test holds `held.bin` too, and is not listed.
        assert_eq!(holders(&while_held, &path), [Pid(holder.id())], "{name}");
        assert_eq!(holders(&after_exit, &path), [], "{name}");
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
