use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use std::path::{Path, PathBuf};
use std::process::Stdio;

fn dustpan() -> Command {
    cargo_bin_cmd!("dustpan")
}

#[test]
fn version_prints_two_lines() {
    let out = dustpan().arg("--version").assert().success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 2, "{stdout}");
    assert_eq!(lines[0], "dustpan 0.1.0");
    assert_eq!(lines[1].split(" · ").count(), 3, "{}", lines[1]);
}

#[test]
fn short_version_flag_matches_long() {
    let long = dustpan().arg("--version").output().unwrap().stdout;
    let short = dustpan().arg("-V").output().unwrap().stdout;
    assert_eq!(long, short);
}

#[test]
fn version_survives_a_closed_pipe() {
    let mut child = std::process::Command::new(assert_cmd::cargo::cargo_bin!("dustpan"))
        .arg("--version")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    assert!(
        out.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn help_lists_the_global_flags_and_no_commands() {
    dustpan()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--debug"))
        .stdout(predicate::str::contains("--ascii"))
        .stdout(predicate::str::contains("--no-color"))
        .stdout(predicate::str::contains("Commands:").not());
}

#[test]
fn bare_invocation_prints_help_and_exits_zero() {
    dustpan()
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage: dustpan"));
}

#[test]
fn unknown_command_exits_2_and_names_it() {
    dustpan()
        .arg("frobnicate")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("frobnicate"));
}

#[test]
fn unknown_flag_exits_2_and_names_it() {
    dustpan()
        .arg("--frob")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("--frob"));
}

#[test]
fn no_color_flag_and_env_strip_escapes_from_help() {
    let colored = dustpan()
        .env("CLICOLOR_FORCE", "1")
        .env_remove("NO_COLOR")
        .arg("--help")
        .output()
        .unwrap()
        .stdout;
    assert!(
        colored.contains(&0x1b),
        "positive control: forced color has escapes"
    );

    let flag = dustpan()
        .env("CLICOLOR_FORCE", "1")
        .env_remove("NO_COLOR")
        .args(["--no-color", "--help"])
        .output()
        .unwrap()
        .stdout;
    assert!(!flag.contains(&0x1b));

    let env = dustpan()
        .env("CLICOLOR_FORCE", "1")
        .env("NO_COLOR", "1")
        .arg("--help")
        .output()
        .unwrap()
        .stdout;
    assert!(!env.contains(&0x1b));
}

fn files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            files_under(&path, out);
        } else {
            out.push(path);
        }
    }
}

#[test]
fn debug_writes_a_session_log_that_names_no_home() {
    let home =
        PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("home-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    dustpan()
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("LOCALAPPDATA", home.join("AppData"))
        .env_remove("XDG_STATE_HOME")
        .args(["--debug", "--version"])
        .assert()
        .success();

    let mut logs = Vec::new();
    files_under(&home, &mut logs);
    assert_eq!(logs.len(), 1, "{logs:?}");
    let log = std::fs::read_to_string(&logs[0]).unwrap();
    assert!(log.starts_with("dustpan 0.1.0\n"), "{log}");
    assert!(!log.contains(home.to_str().unwrap()), "{log}");
    assert!(log.contains("started: 20"), "{log}");
}
