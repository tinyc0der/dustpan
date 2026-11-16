//! The live sink lanes: the real guard, probe, disk, clock and log over fixture trees this test
//! builds. No CLI command reaches a sink in M0 (`docs/M0-checklist.md` Appendix B), so these
//! tests are how the sinks run for real.
//!
//! Each test builds its own home folder under `DUSTPAN_SINK_FIXTURE_HOME/<test>` when that is
//! set, and leaves it there for `dustpan history` to read; the test folder must not exist yet,
//! so the variable cannot point a run at existing data. Otherwise the home is a new temporary
//! folder.
//!
//! The lanes that leave the test's own folders are `#[ignore]`d and run by hand:
//! `trash_lane` moves its fixture to the real Trash, `mount_lane` attaches a disk image.
#![cfg(unix)]

use dustpan_core::candidate::{Candidate, GroupId, Rule, RuleId, Sink};
use dustpan_core::guard::{ContentsReason, Guard, GuardInputs, Homes, KeptReason, Lease};
use dustpan_core::history::{History, Limit};
use dustpan_core::oplog::{Command, LogWarning, OpLog, RunId};
use dustpan_core::path::AbsPath;
use dustpan_core::paths::{self, Os};
use dustpan_core::probe::{Probe, Seen};
use dustpan_core::sink::{Cancel, Executor, Mode, Outcome, Services, Tally};
use dustpan_core::size::Size;
use dustpan_core::time::Clock;
use dustpan_core::whitelist::Whitelist;
use dustpan_sys::{FileStore, LiveDisk, LiveProbe, SystemClock};
use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

struct Fixture {
    home: PathBuf,
    _temp: Option<tempfile::TempDir>,
}

impl Fixture {
    fn new(test: &str) -> Self {
        let (base, temp) = match std::env::var_os("DUSTPAN_SINK_FIXTURE_HOME") {
            Some(base) => {
                std::fs::create_dir_all(&base).unwrap();
                (PathBuf::from(base), None)
            }
            None => {
                // Outside the repository, which an editor may watch and so hold open.
                let temp = tempfile::Builder::new()
                    .prefix("dustpan-sink-")
                    .suffix(".noindex")
                    .tempdir()
                    .unwrap();
                (temp.path().to_path_buf(), Some(temp))
            }
        };
        let home = base.canonicalize().unwrap().join(test);
        assert!(
            !home.exists(),
            "{} exists; pick a new folder",
            home.display()
        );
        std::fs::create_dir_all(home.join("Library/Caches")).unwrap();
        Self { home, _temp: temp }
    }

    fn at(&self, rest: &str) -> PathBuf {
        self.home.join(rest)
    }

    fn tree(&self, rest: &str, folders: usize, files: usize) {
        for folder in 0..folders {
            let dir = self.at(rest).join(format!("d{folder}"));
            std::fs::create_dir_all(&dir).unwrap();
            for file in 0..files {
                std::fs::write(dir.join(format!("f{file}")), b"dustpan sink fixture").unwrap();
            }
        }
    }

    fn log_dir(&self) -> PathBuf {
        let home = self.home.to_string_lossy().into_owned();
        let env = move |key: &str| (key == "HOME").then(|| home.clone());
        paths::log_dir(Os::current(), &env).unwrap()
    }

    fn guard(&self, probe: &LiveProbe) -> Guard {
        let env = |_: &str| None;
        let home = abs(&self.home);
        let account = dustpan_sys::account_home()
            .ok()
            .flatten()
            .and_then(|path| AbsPath::parse(path.as_os_str()).ok());
        let inputs = GuardInputs {
            os: Os::current(),
            homes: Homes::new(home.clone(), account),
            env: &env,
            whitelist: Whitelist::defaults(Os::current(), &home),
        };
        Guard::new(inputs, probe).unwrap()
    }
}

fn abs(path: &Path) -> AbsPath {
    AbsPath::parse(path.as_os_str()).unwrap()
}

fn rule(sink: Sink) -> Arc<Rule> {
    Arc::new(Rule {
        id: RuleId("fixture.cache".into()),
        group: GroupId("fixture".into()),
        owners: Box::new([]),
        sink,
        default_selected: true,
    })
}

fn nonce() -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    u64::from(nanos) ^ u64::from(std::process::id())
}

struct Ran {
    outcomes: Vec<Outcome>,
    tally: Tally,
    warnings: Vec<LogWarning>,
}

/// Discovers each path with a fresh pass, then runs one apply-mode session over all of them,
/// logging to the fixture home's log folder, as `clean` will.
fn run(fixture: &Fixture, paths: &[PathBuf], sink: fn() -> Sink) -> Ran {
    let probe = LiveProbe::new();
    let guard = fixture.guard(&probe);
    let mut pass = guard.pass(&probe);
    let plan: Vec<Candidate> = paths
        .iter()
        .map(|path| {
            let path = abs(path);
            let Seen::Found(node) = probe.node(&path) else {
                panic!("fixture has no {path}")
            };
            let candidate = pass.candidate(path, node, &rule(sink())).unwrap();
            println!("discovered {}: {}", candidate.path(), candidate.verdict());
            candidate
        })
        .collect();
    drop(pass);
    let clock = SystemClock::new();
    let cancel = Cancel::default();
    let mut warnings = Vec::new();
    let mut log = OpLog::new(FileStore::new(fixture.log_dir()), |w: &LogWarning| {
        eprintln!("dustpan: {w}");
        warnings.push(w.clone());
    });
    let disk = LiveDisk::new();
    let services = Services {
        guard: &guard,
        probe: &probe,
        clock: &clock,
        lease: Lease::BATCH,
        journal: &mut log,
        cancel: &cancel,
    };
    let id = RunId::new(clock.wall(), std::process::id(), nonce());
    let mut executor = Executor::start(
        Mode::Apply(&disk),
        Command::named("sink-fixture"),
        id,
        services,
    );
    let mut outcomes = Vec::new();
    for candidate in plan {
        let (candidate, outcome) = executor.run(candidate);
        println!("{}: {outcome:?}", candidate.path());
        outcomes.push(outcome);
    }
    let tally = executor.finish();
    drop(log);
    Ran {
        outcomes,
        tally,
        warnings,
    }
}

fn read_history(fixture: &Fixture) -> History {
    let dir = fixture.log_dir();
    let read = |name| std::fs::read(dir.join(name)).unwrap_or_default();
    History::read(&read(FileStore::OLD), &read(FileStore::FILE))
}

#[test]
fn a_run_removes_one_cache_keeps_one_that_holds_an_unreadable_folder_and_logs_both() {
    use std::os::unix::fs::PermissionsExt as _;
    let fixture = Fixture::new("removes-and-keeps");
    fixture.tree("Library/Caches/com.example.app", 20, 50);
    // An unreadable folder keeps its candidate on every OS. Each OS's protection rows are
    // pinned in core's fake-disk tests; Linux has none that can sit inside a cache.
    let locked = fixture.at("Library/Caches/com.example.other/x/locked");
    std::fs::create_dir_all(&locked).unwrap();
    std::fs::write(locked.join("state"), b"kept").unwrap();
    let mode = |bits| std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(bits));
    mode(0o000).unwrap();
    let removed = fixture.at("Library/Caches/com.example.app");
    let kept = fixture.at("Library/Caches/com.example.other");
    let ran = run(&fixture, &[removed.clone(), kept.clone()], || Sink::Delete);
    mode(0o755).unwrap();
    assert!(
        matches!(ran.outcomes[0], Outcome::Removed(Size::Known(bytes)) if bytes >= 1000 * 4096 / 2),
        "{:?}",
        ran.outcomes[0]
    );
    assert!(!removed.exists());
    assert!(
        matches!(
            &ran.outcomes[1],
            Outcome::Kept(KeptReason::Contents(ContentsReason::Holds { .. }))
        ),
        "{:?}",
        ran.outcomes[1]
    );
    assert!(locked.join("state").exists());
    assert_eq!((ran.tally.removed, ran.tally.kept), (1, 1));
    assert!(ran.warnings.is_empty());
    let history = read_history(&fixture);
    let session = history.sessions(Limit::DEFAULT).next().unwrap();
    assert_eq!(session.command, "sink-fixture");
    assert_eq!((session.actions.removed, session.actions.skipped), (1, 1));
    assert!(!session.ended_at.is_empty());
    let statuses: Vec<_> = history
        .deletions(Limit::DEFAULT)
        .map(|d| d.status.clone())
        .collect();
    assert_eq!(statuses, ["holds-kept", "ok"]);
}

#[test]
fn a_file_and_a_link_are_removed_and_the_links_target_stays() {
    let fixture = Fixture::new("file-and-link");
    let file = fixture.at("Library/Caches/blob.bin");
    std::fs::write(&file, vec![7u8; 10_000]).unwrap();
    std::fs::create_dir_all(fixture.at("Documents")).unwrap();
    std::fs::write(fixture.at("Documents/keep.txt"), b"keep").unwrap();
    let link = fixture.at("Library/Caches/com.example.app");
    std::fs::create_dir_all(&link).unwrap();
    std::os::unix::fs::symlink(fixture.at("Documents"), link.join("docs")).unwrap();
    let ran = run(&fixture, &[file.clone(), link.clone()], || Sink::Delete);
    assert!(
        matches!(ran.outcomes[0], Outcome::Removed(Size::Known(n)) if n >= 10_000),
        "{:?}",
        ran.outcomes
    );
    assert!(
        matches!(ran.outcomes[1], Outcome::Removed(_)),
        "{:?}",
        ran.outcomes
    );
    assert!(!file.exists() && !link.exists());
    assert!(fixture.at("Documents/keep.txt").exists());
}

#[test]
fn a_read_only_log_folder_warns_once_and_the_delete_still_happens() {
    use std::os::unix::fs::PermissionsExt as _;
    let fixture = Fixture::new("read-only-log");
    fixture.tree("Library/Caches/com.example.one", 2, 3);
    fixture.tree("Library/Caches/com.example.two", 2, 3);
    let dir = fixture.log_dir();
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o500)).unwrap();
    let targets = [
        fixture.at("Library/Caches/com.example.one"),
        fixture.at("Library/Caches/com.example.two"),
    ];
    let ran = run(&fixture, &targets, || Sink::Delete);
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(ran.warnings.len(), 1, "{:?}", ran.warnings);
    assert_eq!(ran.tally.removed, 2, "{:?}", ran.outcomes);
    assert!(targets.iter().all(|t| !t.exists()));
    assert!(!dir.join(FileStore::FILE).exists());
}

#[test]
fn a_linked_log_folder_warns_once_logs_nothing_and_the_delete_still_happens() {
    let fixture = Fixture::new("linked-log-folder");
    fixture.tree("Library/Caches/com.example.one", 2, 3);
    let elsewhere = fixture.at("elsewhere");
    std::fs::create_dir(&elsewhere).unwrap();
    let dir = fixture.log_dir();
    std::fs::create_dir_all(dir.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&elsewhere, &dir).unwrap();
    let target = fixture.at("Library/Caches/com.example.one");
    let ran = run(&fixture, std::slice::from_ref(&target), || Sink::Delete);
    assert_eq!(ran.warnings.len(), 1, "{:?}", ran.warnings);
    assert_eq!(ran.tally.removed, 1, "{:?}", ran.outcomes);
    assert!(!target.exists());
    let logged: Vec<_> = std::fs::read_dir(&elsewhere).unwrap().collect();
    assert!(logged.is_empty(), "{logged:?}");
}

fn mkfifo(path: &Path) {
    let made = std::process::Command::new("mkfifo")
        .arg(path)
        .status()
        .unwrap();
    assert!(made.success(), "mkfifo {}", path.display());
}

/// Runs two deletes on a thread of their own after `block` has set up the log folder, so a log
/// call that blocks fails the test instead of hanging it. `block` returns what must stay alive
/// for the run, such as a held lock.
fn a_blocking_log_warns_once_and_the_delete_still_happens<T>(
    test: &str,
    block: impl FnOnce(&Path) -> T,
) {
    let fixture = Fixture::new(test);
    fixture.tree("Library/Caches/com.example.one", 2, 3);
    fixture.tree("Library/Caches/com.example.two", 1, 1);
    let dir = fixture.log_dir();
    std::fs::create_dir_all(&dir).unwrap();
    let _held = block(&dir);
    let targets = vec![
        fixture.at("Library/Caches/com.example.one"),
        fixture.at("Library/Caches/com.example.two"),
    ];
    let (done, finished) = mpsc::channel();
    let paths = targets.clone();
    std::thread::spawn(move || {
        let ran = run(&fixture, &paths, || Sink::Delete);
        let _ = done.send((fixture, ran));
    });
    let (_fixture, ran) = finished
        .recv_timeout(Duration::from_secs(20))
        .expect("the run finishes in time");
    assert_eq!(ran.warnings.len(), 1, "{:?}", ran.warnings);
    assert_eq!(ran.tally.removed, 2, "{:?}", ran.outcomes);
    assert!(targets.iter().all(|t| !t.exists()));
}

#[test]
fn a_fifo_at_the_log_warns_once_and_the_delete_still_happens() {
    a_blocking_log_warns_once_and_the_delete_still_happens("fifo-log", |dir| {
        mkfifo(&dir.join(FileStore::FILE));
    });
}

#[test]
fn a_fifo_at_the_lock_warns_once_and_the_delete_still_happens() {
    a_blocking_log_warns_once_and_the_delete_still_happens("fifo-lock", |dir| {
        mkfifo(&dir.join(FileStore::LOCK));
    });
}

#[test]
fn a_lock_another_run_holds_warns_once_and_the_delete_still_happens() {
    a_blocking_log_warns_once_and_the_delete_still_happens("held-lock", |dir| {
        let lock = std::fs::File::create(dir.join(FileStore::LOCK)).unwrap();
        lock.lock().unwrap();
        lock
    });
}

/// Lanes 5, 6 and 10: moves a uniquely named fixture folder to the real Trash of the user
/// running the test, and prints its name and where it landed.
#[test]
#[ignore = "moves a fixture folder to the real Trash; run by hand for live lanes 5, 6 and 10"]
fn trash_lane() {
    let fixture = Fixture::new(&format!("trash-{:x}", nonce()));
    let name = format!("dustpan-m0-5-lane5-{:x}", nonce());
    let item = fixture.at(&format!("Library/Caches/{name}"));
    fixture.tree(&format!("Library/Caches/{name}"), 1, 2);
    let ran = run(&fixture, std::slice::from_ref(&item), || Sink::Trash);
    println!("lane item {name}");
    assert!(
        matches!(ran.outcomes[0], Outcome::Trashed(_)),
        "{:?}",
        ran.outcomes
    );
    assert_eq!(ran.tally.freed, Size::Known(0));
    assert!(!item.exists());
}

/// The Trash sink against a parent folder swapped for a symlink while it runs: the folder is
/// renamed aside and a link to a victim folder takes its name, at a moment spread over the
/// sink's own duration. The victim holds an entry with the candidate's name and was never
/// approved, so it must never reach the Trash. Run it with `DUSTPAN_SINK_FIXTURE_HOME` on a
/// scratch volume, since the Trash it fills is that volume's. `DUSTPAN_RACE_ATTEMPTS` sets the
/// number of attempts (2000 by default).
#[test]
#[ignore = "fills a volume Trash; run by hand with the fixture home on a scratch disk image"]
#[expect(
    clippy::disallowed_methods,
    reason = "the race swaps and clears the fixture's own folders"
)]
fn trash_race_lane() {
    use std::time::{Duration, Instant};
    let fixture = Fixture::new(&format!("race-{:x}", nonce()));
    let victims = fixture.at("victims");
    let parent = fixture.at("Library/Caches/parent");
    let aside = fixture.at("Library/Caches/parent.aside");
    let item = parent.join("com.example.item");
    let victim = victims.join("com.example.item");
    let attempts: u32 = std::env::var("DUSTPAN_RACE_ATTEMPTS")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(2000);
    let probe = LiveProbe::new();
    let guard = fixture.guard(&probe);
    let clock = SystemClock::new();
    let cancel = Cancel::default();
    let mut log = OpLog::new(FileStore::new(fixture.log_dir()), |w: &LogWarning| {
        eprintln!("dustpan: {w}");
    });
    let disk = LiveDisk::new();
    let services = Services {
        guard: &guard,
        probe: &probe,
        clock: &clock,
        lease: Lease::BATCH,
        journal: &mut log,
        cancel: &cancel,
    };
    let id = RunId::new(clock.wall(), std::process::id(), nonce());
    let mut executor = Executor::start(Mode::Apply(&disk), Command::named("race"), id, services);
    let mut outcomes = std::collections::BTreeMap::<String, u32>::new();
    let mut typical = Duration::MAX;
    let mut hits = 0;
    for n in 0..attempts {
        std::fs::create_dir_all(item.join("d")).unwrap();
        std::fs::write(item.join("d/f"), b"approved").unwrap();
        std::fs::create_dir_all(&victim).unwrap();
        std::fs::write(victim.join("precious.txt"), b"never approved").unwrap();
        let path = abs(&item);
        let Seen::Found(node) = probe.node(&path) else {
            panic!("fixture has no {path}")
        };
        let candidate = guard
            .pass(&probe)
            .candidate(path, node, &rule(Sink::Trash))
            .unwrap();
        // The first rounds only measure how long one item takes; the rest swap the parent at
        // a point spread from the start of the run to just past its end.
        let swap_at = (n >= 5).then(|| typical.mul_f64(f64::from(n % 100) / 100.0 * 1.1));
        let (from, to, link) = (parent.clone(), aside.clone(), victims.clone());
        let started = Instant::now();
        let swapper = std::thread::spawn(move || {
            let Some(after) = swap_at else { return false };
            while started.elapsed() < after {
                std::hint::spin_loop();
            }
            std::fs::rename(&from, &to).unwrap();
            std::os::unix::fs::symlink(&link, &from).unwrap();
            true
        });
        let (_, outcome) = executor.run(candidate);
        let took = started.elapsed();
        let swapped = swapper.join().unwrap();
        if n < 5 {
            typical = typical.min(took);
        }
        let victim_moved = !victim.join("precious.txt").exists();
        hits += u32::from(victim_moved);
        let word = match &outcome {
            Outcome::Failed(f) => format!("failed {}", f.cause.code()),
            Outcome::Kept(r) => format!("kept {}", r.code()),
            other => format!("{other:?}").split('(').next().unwrap().to_owned(),
        };
        *outcomes
            .entry(format!("{word}, victim moved: {victim_moved}"))
            .or_default() += 1;
        if swapped {
            std::fs::remove_file(&parent).unwrap();
            std::fs::rename(&aside, &parent).unwrap();
        }
        if item.exists() {
            std::fs::remove_dir_all(&item).unwrap();
        }
        if victim.exists() {
            std::fs::remove_dir_all(&victim).unwrap();
        }
    }
    executor.finish();
    drop(log);
    println!("one item takes about {typical:?}");
    for (outcome, count) in &outcomes {
        println!("{count:5}  {outcome}");
    }
    println!("victim moved to the Trash in {hits} of {attempts} attempts");
    assert_eq!(hits, 0);
}

/// The folder at the top of the volume `path` is on: the last ancestor on the same device.
#[cfg(target_os = "macos")]
fn mount_point(path: &Path) -> PathBuf {
    use std::os::unix::fs::MetadataExt as _;
    let device = std::fs::metadata(path).unwrap().dev();
    path.ancestors()
        .take_while(|up| std::fs::metadata(up).is_ok_and(|meta| meta.dev() == device))
        .last()
        .unwrap()
        .to_path_buf()
}

/// SAFE-12 and SAFE-13: the user's Trash on the item's volume (`.Trashes/<uid>`) is a link to
/// another folder. The move is refused as `trash-failed`, the item stays, and nothing reaches
/// the link's target. Run it with `DUSTPAN_SINK_FIXTURE_HOME` on a scratch disk image, whose
/// Trash it changes and then restores.
#[cfg(target_os = "macos")]
#[test]
#[ignore = "swaps a volume's Trash for a link; run by hand with the fixture home on a scratch disk image"]
#[expect(
    clippy::disallowed_methods,
    reason = "the test moves the scratch volume's own Trash aside and back"
)]
fn trash_symlink_lane() {
    let fixture = Fixture::new(&format!("symlinked-trash-{:x}", nonce()));
    let volume = mount_point(&fixture.home);
    assert_ne!(volume, Path::new("/"), "needs a scratch volume");
    // SAFETY: getuid takes no arguments and cannot fail.
    let uid = unsafe { libc::getuid() };
    let trashes = volume.join(".Trashes");
    let trash = trashes.join(uid.to_string());
    let aside = trashes.join(format!("{uid}.aside-{:x}", nonce()));
    let elsewhere = volume.join(format!("elsewhere-{:x}", nonce()));
    std::fs::create_dir_all(&trash).unwrap();
    std::fs::create_dir_all(&elsewhere).unwrap();
    std::fs::rename(&trash, &aside).unwrap();
    std::os::unix::fs::symlink(&elsewhere, &trash).unwrap();
    let name = "com.example.symlinked-trash";
    let item = fixture.at(&format!("Library/Caches/{name}"));
    fixture.tree(&format!("Library/Caches/{name}"), 1, 2);
    let ran = run(&fixture, std::slice::from_ref(&item), || Sink::Trash);
    let reached = std::fs::read_dir(&elsewhere).unwrap().count();
    std::fs::remove_file(&trash).unwrap();
    std::fs::rename(&aside, &trash).unwrap();
    println!(
        "{:?}; item in place: {}; entries in the link's target: {reached}",
        ran.outcomes[0],
        item.exists()
    );
    assert!(
        matches!(&ran.outcomes[0], Outcome::Failed(f) if f.cause.code() == "trash-failed"),
        "{:?}",
        ran.outcomes[0]
    );
    assert!(item.join("d0/f0").exists());
    assert_eq!(reached, 0);
}

/// Lane 7: a disk image mounted inside a fixture folder keeps the folder, and the files on the
/// image survive.
#[cfg(target_os = "macos")]
#[test]
#[ignore = "attaches a disk image with hdiutil; run by hand for live lane 7"]
fn mount_lane() {
    let fixture = Fixture::new(&format!("mount-{:x}", nonce()));
    let folder = fixture.at("Library/Caches/com.example.mnt");
    let mountpoint = folder.join("mnt");
    std::fs::create_dir_all(&mountpoint).unwrap();
    std::fs::write(folder.join("beside.txt"), b"beside").unwrap();
    let image = fixture.at("image.dmg");
    let hdiutil = |args: &[&std::ffi::OsStr]| {
        let status = std::process::Command::new("/usr/bin/hdiutil")
            .args(args)
            .stdin(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success(), "hdiutil {args:?}");
    };
    hdiutil(&[
        "create".as_ref(),
        "-size".as_ref(),
        "4m".as_ref(),
        "-fs".as_ref(),
        "HFS+".as_ref(),
        "-volname".as_ref(),
        "dustpan-lane7".as_ref(),
        image.as_os_str(),
    ]);
    hdiutil(&[
        "attach".as_ref(),
        "-nobrowse".as_ref(),
        "-mountpoint".as_ref(),
        mountpoint.as_os_str(),
        image.as_os_str(),
    ]);
    std::fs::write(mountpoint.join("on-the-image.txt"), b"mounted").unwrap();
    let ran = run(&fixture, std::slice::from_ref(&folder), || Sink::Delete);
    let survived = mountpoint.join("on-the-image.txt").exists();
    hdiutil(&["detach".as_ref(), mountpoint.as_os_str()]);
    println!("mounted file survived: {survived}");
    assert!(survived);
    assert!(
        folder.join("beside.txt").exists(),
        "nothing beside the mount was removed"
    );
    assert!(
        matches!(&ran.outcomes[0], Outcome::Kept(KeptReason::Contents(ContentsReason::Holds { reason, .. }))
            if **reason == KeptReason::Shape(dustpan_core::guard::ShapeReason::MountPoint)),
        "{:?}",
        ran.outcomes[0]
    );
}
