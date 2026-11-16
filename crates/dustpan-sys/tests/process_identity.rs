//! The process table ties a running process to its app through the bundle's `Info.plist`, and
//! says so when it cannot (SAFE-08).
#![cfg(target_os = "macos")]

use dustpan_core::probe::{App, Pid, Probe, Seen};
use dustpan_sys::LiveProbe;
use std::path::Path;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

const SLEEP: &str = "DUSTPAN_TEST_SLEEP";
const BUNDLE_ID: &str = "com.dustpan.test.owner";

/// The body of the process the test spawns. A copy of `/bin/sleep` cannot stand in: macOS
/// kills a system binary that runs from another place. A copy of this test binary runs
/// anywhere, and sleeps when it is asked to.
#[test]
fn helper_sleeps_when_asked() {
    if std::env::var_os(SLEEP).is_some() {
        std::thread::sleep(Duration::from_secs(120));
    }
}

struct Running(Child);

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn app_of(pid: u32) -> Option<App> {
    let Seen::Found(table) = LiveProbe::new().processes() else {
        panic!("the process table is readable");
    };
    table
        .procs
        .into_iter()
        .find(|p| p.pid == Pid(pid))
        .map(|p| p.app)
}

/// Polls until the table says `want` about `pid`; the last answer otherwise.
fn settles_on(pid: u32, want: &App) -> Option<App> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let app = app_of(pid);
        if app.as_ref() == Some(want) || Instant::now() > deadline {
            return app;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn write_plist(contents: &Path) {
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>{BUNDLE_ID}</string>
</dict></plist>
"#
    );
    std::fs::write(contents.join("Info.plist"), plist).unwrap();
}

#[test]
fn a_process_is_tied_to_its_bundle_until_its_binary_is_deleted() {
    let dir = tempfile::tempdir().unwrap();
    let contents = dir.path().join("Owner.app/Contents");
    std::fs::create_dir_all(contents.join("MacOS")).unwrap();
    write_plist(&contents);
    let exe = contents.join("MacOS/owner");
    std::fs::copy(std::env::current_exe().unwrap(), &exe).unwrap();

    #[expect(
        clippy::disallowed_methods,
        reason = "runs the copy of this test binary as the process under test"
    )]
    let child = Command::new(&exe)
        .args(["--exact", "helper_sleeps_when_asked"])
        .env(SLEEP, "1")
        .spawn()
        .unwrap();
    let pid = child.id();
    let _running = Running(child);

    let in_bundle = App::Bundles(Box::new([BUNDLE_ID.into()]));
    assert_eq!(settles_on(pid, &in_bundle), Some(in_bundle));

    #[expect(
        clippy::disallowed_methods,
        reason = "the test deletes the one binary it copied into its own temporary folder"
    )]
    std::fs::remove_file(&exe).unwrap();
    let unknown = App::Unknown { mine: true };
    assert_eq!(settles_on(pid, &unknown), Some(unknown));
}

#[test]
fn a_bundle_with_an_unreadable_plist_leaves_its_process_unidentified() {
    let dir = tempfile::tempdir().unwrap();
    let contents = dir.path().join("Broken.app/Contents");
    std::fs::create_dir_all(contents.join("MacOS")).unwrap();
    std::fs::write(contents.join("Info.plist"), "not a property list").unwrap();
    let exe = contents.join("MacOS/broken");
    std::fs::copy(std::env::current_exe().unwrap(), &exe).unwrap();

    #[expect(
        clippy::disallowed_methods,
        reason = "runs the copy of this test binary as the process under test"
    )]
    let child = Command::new(&exe)
        .args(["--exact", "helper_sleeps_when_asked"])
        .env(SLEEP, "1")
        .spawn()
        .unwrap();
    let pid = child.id();
    let _running = Running(child);

    let unknown = App::Unknown { mine: true };
    assert_eq!(settles_on(pid, &unknown), Some(unknown));
}
