use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use std::path::Path;

fn fresh_dir() -> tempfile::TempDir {
    tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap()
}

/// A home folder with a head folder and a file in it. Unix homes come from the shared fixture
/// script; Windows has no bash on every runner, so it builds the two entries it needs.
fn fixture_home() -> tempfile::TempDir {
    let home = fresh_dir();
    if cfg!(unix) {
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/mkhome.sh");
        let built = std::process::Command::new("bash")
            .arg(script)
            .arg(home.path())
            .status()
            .unwrap();
        assert!(built.success());
    } else {
        std::fs::create_dir_all(home.path().join("Documents")).unwrap();
        std::fs::write(home.path().join("Documents").join("report.txt"), "x").unwrap();
    }
    home
}

fn explain(home: &Path) -> Command {
    let mut cmd = cargo_bin_cmd!("dustpan");
    cmd.env("HOME", home)
        .env("USERPROFILE", home)
        .current_dir(home)
        .arg("explain");
    cmd
}

fn stdout(cmd: &mut Command) -> String {
    let out = cmd.assert().success();
    String::from_utf8(out.get_output().stdout.clone()).unwrap()
}

/// The spelling `explain` prints for a path under `home`: physical, as the guard judges it.
fn shown(home: &Path, rest: &str) -> String {
    let home = home.canonicalize().unwrap();
    let home = home
        .to_str()
        .unwrap()
        .trim_start_matches(r"\\?\")
        .to_owned();
    format!(
        "{}{}",
        home,
        rest.replace('/', std::path::MAIN_SEPARATOR_STR)
    )
}

#[test]
fn a_head_folder_is_kept_and_a_file_in_it_is_eligible() {
    let home = fixture_home();
    let text = stdout(explain(home.path()).args(["~/Documents", "~/Documents/report.txt"]));
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines,
        [
            format!("{}  Kept · home folder", shown(home.path(), "/Documents")),
            format!("{}  Eligible", shown(home.path(), "/Documents/report.txt")),
        ]
    );
}

#[test]
fn a_relative_path_is_judged_from_the_working_folder() {
    let home = fixture_home();
    let text = stdout(explain(home.path()).arg("Documents/report.txt"));
    assert_eq!(
        text.trim_end(),
        format!("{}  Eligible", shown(home.path(), "/Documents/report.txt"))
    );
}

#[cfg(unix)]
#[test]
fn a_path_through_a_symlink_is_kept_for_its_linked_ancestor() {
    let home = fixture_home();
    let text = stdout(explain(home.path()).arg("~/link/Library"));
    let link = shown(home.path(), "/link");
    assert_eq!(
        text.lines().next().unwrap(),
        format!("{link}/Library  Kept · symlinked ancestor {link}")
    );
}

#[cfg(unix)]
#[test]
fn a_control_byte_is_refused_and_printed_escaped() {
    let home = fixture_home();
    let text = stdout(explain(home.path()).arg("~/bad\nname"));
    assert_eq!(
        text,
        format!(
            "{}  Kept · the name contains a control character\n",
            shown(home.path(), r"/bad\nname")
        )
    );
}

#[cfg(target_os = "macos")]
#[test]
fn macos_roots_are_critical_and_an_idle_cache_is_eligible() {
    let home = fixture_home();
    let text = stdout(explain(home.path()).args([
        "/System",
        "/SYSTEM",
        "/",
        "~/Library/Caches/com.example.app",
        "~/Library/Caches/com.apple.Safari",
    ]));
    let verdicts: Vec<&str> = text
        .lines()
        .take(4)
        .map(|line| line.split_once("  ").unwrap().1)
        .collect();
    assert_eq!(
        verdicts,
        [
            "Kept · critical system path",
            "Kept · critical system path",
            "Kept · system folder",
            "Eligible",
        ]
    );
    // Kept by name, or sooner because Safari is running on this host.
    assert!(text.lines().last().unwrap().contains("  Kept · "), "{text}");
}

#[test]
fn json_is_one_envelope_with_an_item_per_path() {
    let home = fixture_home();
    let text =
        stdout(explain(home.path()).args(["--json", "~/Documents", "~/Documents/report.txt"]));
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["command"], "explain");
    assert_eq!(value["mode"], "read-only");
    assert!(value["run_id"].as_str().is_some_and(|id| !id.is_empty()));
    let items = value["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(
        items[0],
        serde_json::json!({
            "path": shown(home.path(), "/Documents"),
            "verdict": "kept",
            "layer": "critical",
            "reason": "home-folder",
            "detail": "home folder",
            "source": null,
        })
    );
    assert_eq!(
        items[1],
        serde_json::json!({
            "path": shown(home.path(), "/Documents/report.txt"),
            "verdict": "eligible",
            "layer": null,
            "reason": null,
            "detail": null,
            "source": null,
        })
    );
}

#[cfg(unix)]
#[test]
fn json_keeps_a_hostile_name_inside_its_string() {
    let home = fixture_home();
    let text = stdout(explain(home.path()).args(["--json", "~/a\"b\n\"verdict\":\"eligible\""]));
    assert_eq!(text.lines().count(), 1);
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    let item = &value["items"][0];
    assert_eq!(item["verdict"], "kept");
    assert_eq!(item["reason"], "control-byte");
    assert!(
        item["path"]
            .as_str()
            .unwrap()
            .ends_with(r#"/a"b\n"verdict":"eligible""#)
    );
}

#[test]
fn no_path_is_a_usage_error() {
    let home = fresh_dir();
    explain(home.path())
        .assert()
        .code(2)
        .stderr(predicate::str::contains("PATH"));
}

#[test]
fn a_home_that_is_a_system_root_stops_the_command() {
    let root = if cfg!(windows) { r"C:\" } else { "/" };
    explain(Path::new(root))
        .arg(root)
        .assert()
        .code(1)
        .stdout("")
        .stderr(predicate::str::contains(
            "cannot load the protection policy",
        ))
        .stderr(predicate::str::contains("is a protected system path"));
}

#[test]
fn an_unset_home_stops_the_command() {
    let dir = fresh_dir();
    cargo_bin_cmd!("dustpan")
        .env_remove("HOME")
        .env_remove("USERPROFILE")
        .current_dir(dir.path())
        .args(["explain", "x"])
        .assert()
        .code(1)
        .stdout("")
        .stderr(predicate::str::contains("home folder is not set"));
}
