#![cfg(target_os = "macos")]

use dustpan_parity::error::Error;
use dustpan_parity::fixture_home::{Fixture, Manifest, assert_under_temp};
use dustpan_parity::mole_runner::{MoleRunner, real_home, sandbox_profile};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

const WRITE_DENY_ONLY: &str = include_str!("deny-writes.sb");
const ALLOW_ALL: &str = "(version 1)(allow default)";
const TRUE: &str = "/usr/bin/true";

fn fixture() -> Fixture {
    let manifest = Manifest::parse("[[file]]\npath = \"home/a.txt\"\nsize = 1\n").unwrap();
    Fixture::build(&manifest).unwrap()
}

fn build(profile: &str) -> Result<MoleRunner, Error> {
    MoleRunner::with_profile(
        PathBuf::from(TRUE),
        &fixture(),
        Duration::from_secs(30),
        &real_home().unwrap(),
        profile,
    )
}

fn leftover_probes() -> Vec<String> {
    fs::read_dir(real_home().unwrap())
        .unwrap()
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.starts_with(".dustpan-parity-probe-"))
        .collect()
}

fn refusal(profile: &str) -> String {
    match build(profile) {
        Err(Error::Sandbox(m)) => m,
        Err(other) => panic!("expected a sandbox refusal, got {other}"),
        Ok(_) => panic!("the self-test accepted the profile {profile:?}"),
    }
}

#[test]
fn an_allow_all_profile_fails_the_self_test_and_leaves_no_probe_behind() {
    let message = refusal(ALLOW_ALL);
    assert!(message.contains("was not denied"), "{message}");
    assert!(leftover_probes().is_empty(), "{:?}", leftover_probes());
}

#[test]
fn a_profile_that_denies_writes_but_not_reads_of_the_real_home_fails() {
    let message = refusal(WRITE_DENY_ONLY);
    assert!(message.contains("reading"), "{message}");
}

#[test]
fn the_generated_profile_passes_and_denies_the_real_home() {
    let home = real_home().unwrap();
    let profile = sandbox_profile(&home, Path::new(TRUE)).unwrap();
    assert!(profile.contains(&format!(
        "(deny file-read-data (subpath \"{}\"))",
        home.display()
    )));
    assert!(build(&profile).is_ok());
    assert!(leftover_probes().is_empty());
}

#[test]
fn a_real_home_is_never_a_temp_root() {
    let err = assert_under_temp(&real_home().unwrap()).unwrap_err();
    assert!(matches!(err, Error::FixtureOutsideTemp(_)));
}

#[test]
fn a_stop_signal_kills_mole_and_removes_the_fixture() {
    let work = tempfile::Builder::new()
        .prefix("dustpan-signal-test.")
        .tempdir_in("/private/tmp")
        .unwrap();
    let marker = work.path().join("pid");
    let mo = work.path().join("mo");
    fs::write(
        &mo,
        format!(
            "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'Mole version 1.67.0'; exit 0; fi\necho $$ > {}\nexec /bin/sleep 120\n",
            marker.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&mo, fs::Permissions::from_mode(0o755)).unwrap();
    let mut harness = Command::new(env!("CARGO_BIN_EXE_parity"))
        .arg("purge")
        .env("PARITY_MO", &mo)
        .env("TMPDIR", work.path())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let started = Instant::now();
    while !marker.exists() {
        assert!(
            started.elapsed() < Duration::from_secs(60),
            "Mole never started"
        );
        sleep(Duration::from_millis(50));
    }
    sleep(Duration::from_millis(200));
    let mole_pid = fs::read_to_string(&marker).unwrap().trim().to_string();
    let signal = |sig: &str, pid: &str| {
        Command::new("/bin/kill")
            .args([sig, pid])
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success()
    };
    assert!(
        signal("-0", &mole_pid),
        "Mole is not running before the stop"
    );
    assert!(signal("-TERM", &harness.id().to_string()));
    let status = harness.wait().unwrap();
    assert_eq!(status.code(), Some(130));
    assert!(
        !signal("-0", &mole_pid),
        "Mole {mole_pid} outlived the harness"
    );
    let left: Vec<_> = fs::read_dir(work.path())
        .unwrap()
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.starts_with("dustpan-parity"))
        .collect();
    assert!(left.is_empty(), "fixture left behind: {left:?}");
}
