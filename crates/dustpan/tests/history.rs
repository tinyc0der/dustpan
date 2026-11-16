//! `dustpan history` against a log in a fixture home: Mole's fields, the limit, and that it
//! never writes.
use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin_cmd;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn home() -> tempfile::TempDir {
    tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap()
}

fn log_dir(home: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        home.join("Library/Logs/dustpan")
    } else if cfg!(windows) {
        home.join("AppData/Local/dustpan/logs")
    } else {
        home.join(".local/state/dustpan")
    }
}

fn history(home: &Path) -> Command {
    let mut cmd = cargo_bin_cmd!("dustpan");
    cmd.env("HOME", home)
        .env("USERPROFILE", home)
        .env("LOCALAPPDATA", home.join("AppData/Local"))
        .env_remove("XDG_STATE_HOME")
        .arg("history");
    cmd
}

const LOG: &[u8] = b"\n# ========== clean run=20261116094500-42-9f3a session started at 2026-11-16 09:45:00 +0700 ==========\n\
[2026-11-16 09:45:01] [clean run=20261116094500-42-9f3a] REMOVED /home/a/.cache/x (sink=delete size=1500000)\n\
[2026-11-16 09:45:01] [clean run=20261116094500-42-9f3a] SKIPPED /home/a/.cache/y (sink=trash reason=owner-running)\n\
# ========== clean run=20261116094500-42-9f3a session ended at 2026-11-16 09:45:02, 1 items, 1.5 MB ==========\n";

fn write_log(home: &Path, bytes: &[u8]) -> PathBuf {
    let dir = log_dir(home);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("operations.log");
    std::fs::write(&file, bytes).unwrap();
    file
}

fn json(home: &Path, args: &[&str]) -> serde_json::Value {
    let out = history(home).arg("--json").args(args).assert().success();
    serde_json::from_slice(&out.get_output().stdout).unwrap()
}

#[test]
fn json_carries_moles_fields_and_the_envelope_keys() {
    let home = home();
    write_log(home.path(), LOG);
    let v = json(home.path(), &[]);
    let keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    for key in [
        "schema_version",
        "command",
        "logs",
        "limit",
        "sessions",
        "deletions",
    ] {
        assert!(keys.contains(&key), "{key} in {keys:?}");
    }
    assert_eq!(v["command"], "history");
    assert_eq!(v["limit"], 20);
    let session = &v["sessions"][0];
    assert_eq!(session["command"], "clean");
    assert_eq!(session["run_id"], "20261116094500-42-9f3a");
    assert_eq!(session["attribution"], "run");
    assert_eq!(session["started_at"], "2026-11-16 09:45:00");
    assert_eq!(session["ended_at"], "2026-11-16 09:45:02");
    assert_eq!(session["items"], 1);
    assert_eq!(session["size"], "1.5 MB");
    assert_eq!(session["operation_count"], 2);
    assert_eq!(session["failed_tasks"], 0);
    assert_eq!(session["actions"]["removed"], 1);
    assert_eq!(session["actions"]["skipped"], 1);
    let deletions = v["deletions"].as_array().unwrap();
    assert_eq!(deletions[0]["status"], "owner-running");
    assert_eq!(deletions[0]["mode"], "trash");
    assert_eq!(deletions[0]["size_kb"], serde_json::Value::Null);
    assert_eq!(deletions[1]["timestamp"], "2026-11-16T09:45:01+0700");
    assert_eq!(deletions[1]["mode"], "permanent");
    assert_eq!(deletions[1]["size_kb"], 1465);
}

#[test]
fn a_limit_outside_one_to_two_hundred_is_a_usage_error() {
    let home = home();
    for limit in ["0", "201", "-1", "x"] {
        history(home.path())
            .args(["--limit", limit])
            .assert()
            .code(2);
    }
    assert_eq!(json(home.path(), &["--limit", "200"])["limit"], 200);
}

#[test]
fn the_limit_caps_both_lists() {
    let home = home();
    let mut log = Vec::new();
    for i in 0..5 {
        log.extend_from_slice(
            format!(
                "[2026-11-16 09:00:0{i}] [clean run={i}-1-1] REMOVED /x/{i} (sink=delete size=1)\n"
            )
            .as_bytes(),
        );
    }
    write_log(home.path(), &log);
    let v = json(home.path(), &["--limit", "2"]);
    assert_eq!(v["sessions"].as_array().unwrap().len(), 2);
    assert_eq!(v["deletions"].as_array().unwrap().len(), 2);
    assert_eq!(v["deletions"][0]["path"], "/x/4");
}

#[test]
fn invalid_utf8_reads_as_the_replacement_character_and_the_log_is_unchanged() {
    let home = home();
    let mut log = LOG.to_vec();
    log.extend_from_slice(b"[2026-11-16 09:45:03] [clean run=20261116094500-42-9f3a] REMOVED /bad/\xff\xfe (sink=delete size=1)\n");
    let file = write_log(home.path(), &log);
    let before = std::fs::metadata(&file).unwrap().modified().unwrap();
    let v = json(home.path(), &[]);
    assert_eq!(v["deletions"][0]["path"], "/bad/\u{fffd}\u{fffd}");
    let text = history(home.path()).assert().success();
    let text = String::from_utf8(text.get_output().stdout.clone()).unwrap();
    assert!(text.contains("/bad/\u{fffd}\u{fffd}"), "{text}");
    assert_eq!(std::fs::read(&file).unwrap(), log);
    assert_eq!(
        std::fs::metadata(&file).unwrap().modified().unwrap(),
        before
    );
}

#[test]
fn no_log_is_an_empty_history_and_creates_nothing() {
    let home = home();
    let v = json(home.path(), &[]);
    assert_eq!(v["sessions"], serde_json::json!([]));
    assert!(!log_dir(home.path()).exists());
}

#[test]
fn a_control_byte_in_the_log_is_escaped_in_text() {
    let home = home();
    write_log(
        home.path(),
        b"[2026-11-16 09:45:03] [clean run=1-1-1] REMOVED /x/\x1b[31mred (sink=delete size=1)\n",
    );
    let out = history(home.path()).assert().success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(text.contains(r"/x/\x1B[31mred"), "{text}");
    assert!(!text.contains('\x1b'));
}

#[cfg(unix)]
#[test]
fn a_relative_home_is_refused_and_no_log_folder_is_named() {
    let cwd = home();
    let out = cargo_bin_cmd!("dustpan")
        .current_dir(cwd.path())
        .env("HOME", "rel")
        .env_remove("XDG_STATE_HOME")
        .arg("history")
        .assert()
        .code(1);
    let stderr = String::from_utf8_lossy(&out.get_output().stderr);
    assert!(stderr.contains("absolute"), "{stderr}");
    assert!(out.get_output().stdout.is_empty());
}

/// Fails after `limit` instead of hanging, if the read blocks.
fn history_within(home: &Path, limit: Duration) -> Command {
    let mut cmd = history(home);
    cmd.timeout(limit);
    cmd
}

#[cfg(unix)]
#[test]
fn a_fifo_at_the_log_exits_1_and_names_the_file() {
    let home = home();
    let dir = log_dir(home.path());
    std::fs::create_dir_all(&dir).unwrap();
    let fifo = dir.join("operations.log");
    let made = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .unwrap();
    assert!(made.success());
    let out = history_within(home.path(), Duration::from_secs(10))
        .assert()
        .code(1);
    let stderr = String::from_utf8_lossy(&out.get_output().stderr);
    assert!(stderr.contains(&*fifo.to_string_lossy()), "{stderr}");
    assert!(stderr.contains("not a regular file"), "{stderr}");
}

#[test]
fn a_log_over_the_read_cap_exits_1_and_names_the_file() {
    let home = home();
    let file = write_log(home.path(), LOG);
    let big = std::fs::OpenOptions::new().write(true).open(&file).unwrap();
    big.set_len(64 * 1024 * 1024 + 1).unwrap();
    let out = history_within(home.path(), Duration::from_secs(30))
        .assert()
        .code(1);
    let stderr = String::from_utf8_lossy(&out.get_output().stderr);
    assert!(stderr.contains(&*file.to_string_lossy()), "{stderr}");
    assert!(stderr.contains("64 MiB"), "{stderr}");
}
