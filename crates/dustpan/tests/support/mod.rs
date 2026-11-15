//! A fixture home, a process that runs from a folder the test lays out, and `explain` under a
//! deadline, for tests of who owns a cache.
use assert_cmd::cargo::cargo_bin;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

const SLEEP: &str = "DUSTPAN_TEST_SLEEP";

/// The body of the `helper_sleeps_when_asked` test each test binary declares, which is what a
/// copy of that binary runs. A copy of `/bin/sleep` cannot stand in: macOS kills a system
/// binary that runs from another place.
pub fn sleep_if_asked() {
    if std::env::var_os(SLEEP).is_some() {
        std::thread::sleep(Duration::from_secs(120));
    }
}

pub struct Running(Child);

impl Running {
    pub fn pid(&self) -> u32 {
        self.0.id()
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// `dustpan explain path`, killed when it is still running after `limit`.
fn explain_within(limit: Duration, home: &Path, path: &str) -> Option<Output> {
    let mut child = Command::new(cargo_bin("dustpan"))
        .env("HOME", home)
        .args(["explain", path])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + limit;
    while child.try_wait().unwrap().is_none() {
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Some(child.wait_with_output().unwrap())
}

/// A fresh folder with a home in it. `root` is its physical path, as `explain` prints it.
pub struct Fixture {
    _dir: tempfile::TempDir,
    pub root: PathBuf,
}

impl Fixture {
    pub fn new() -> Self {
        let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
        let root = dir.path().canonicalize().unwrap();
        std::fs::create_dir_all(root.join("home")).unwrap();
        Self { _dir: dir, root }
    }

    fn home(&self) -> PathBuf {
        self.root.join("home")
    }

    /// Creates `rest` as a folder in the home and returns its path.
    pub fn home_dir(&self, rest: &str) -> String {
        let dir = self.home().join(rest);
        std::fs::create_dir_all(&dir).unwrap();
        dir.to_str().unwrap().to_owned()
    }

    /// Copies this test binary to `exe`, below the fixture, and leaves it running there, asleep.
    ///
    /// The system checks a binary the first time it runs, and a process snapshot taken during
    /// that check waited 0.5 to 0.9 seconds on the bench host. One untimed `explain` waits it
    /// out, so the timed calls measure dustpan.
    pub fn run(&self, exe: &str) -> Running {
        let exe = self.root.join(exe);
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::copy(std::env::current_exe().unwrap(), &exe).unwrap();
        let child = Command::new(exe)
            .args(["--exact", "helper_sleeps_when_asked"])
            .env(SLEEP, "1")
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let running = Running(child);
        let settle = self.home().join("Library/Caches/com.dustpan.test.settle");
        let settled = explain_within(
            Duration::from_secs(30),
            &self.home(),
            settle.to_str().unwrap(),
        );
        assert!(settled.is_some(), "explain did not return in 30 seconds");
        running
    }

    /// What `explain` says about `path`: the text after the path. Panics when `explain` is
    /// still running after a second.
    pub fn explain(&self, path: &str) -> String {
        let out = explain_within(Duration::from_secs(1), &self.home(), path)
            .unwrap_or_else(|| panic!("explain {path} did not return within a second"));
        let text = String::from_utf8(out.stdout).unwrap();
        let verdict = text.trim_end().strip_prefix(path);
        verdict
            .unwrap_or_else(|| panic!("unexpected output: {text}"))
            .trim_start()
            .to_owned()
    }
}

pub fn plist(bundle_id: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>{bundle_id}</string>
</dict></plist>
"#
    )
}
