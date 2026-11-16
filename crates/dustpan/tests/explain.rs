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
        .env("APPDATA", home.join("AppData"))
        .env_remove("XDG_CONFIG_HOME")
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

/// What `explain` says about a path that no rule keeps and no process holds open. Windows
/// cannot list open files before M8, so there the path is kept and the reason says so.
const FREE: &str = if cfg!(windows) {
    "Kept · cannot tell whether a process has it open: this build cannot check it yet"
} else {
    "Eligible"
};

#[test]
fn a_head_folder_is_kept_and_a_file_in_it_is_eligible() {
    let home = fixture_home();
    let text = stdout(explain(home.path()).args(["~/Documents", "~/Documents/report.txt"]));
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines,
        [
            format!("{}  Kept · home folder", shown(home.path(), "/Documents")),
            format!("{}  {FREE}", shown(home.path(), "/Documents/report.txt")),
        ]
    );
}

#[test]
fn a_relative_path_is_judged_from_the_working_folder() {
    let home = fixture_home();
    let text = stdout(explain(home.path()).arg("Documents/report.txt"));
    assert_eq!(
        text.trim_end(),
        format!("{}  {FREE}", shown(home.path(), "/Documents/report.txt"))
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
    let report = shown(home.path(), "/Documents/report.txt");
    let free = if cfg!(windows) {
        serde_json::json!({
            "path": report,
            "verdict": "kept",
            "layer": "live",
            "reason": "file-open-unknown",
            "detail": FREE.strip_prefix("Kept · "),
            "source": null,
        })
    } else {
        serde_json::json!({
            "path": report,
            "verdict": "eligible",
            "layer": null,
            "reason": null,
            "detail": null,
            "source": null,
        })
    };
    assert_eq!(items[1], free);
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

#[cfg(unix)]
fn write_whitelist(home: &Path, body: &str) -> std::path::PathBuf {
    let dir = home.join(".config").join("dustpan");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("whitelist");
    std::fs::write(&file, body).unwrap();
    file
}

#[cfg(unix)]
#[test]
fn a_default_whitelist_entry_keeps_a_gradle_cache() {
    let home = fixture_home();
    let out = stdout(explain(home.path()).arg("~/.gradle/caches/x"));
    assert_eq!(
        out,
        format!(
            "{}  Kept · protected by default whitelist\n",
            shown(home.path(), "/.gradle/caches/x")
        )
    );
}

#[cfg(unix)]
#[test]
fn a_user_entry_keeps_its_children_and_the_output_names_the_file() {
    let home = fixture_home();
    let file = write_whitelist(home.path(), "~/Projects/keep\r\n");
    let out = stdout(explain(home.path()).args(["~/Projects/keep/a", "~/Projects/webapp"]));
    let lines: Vec<&str> = out.lines().collect();
    assert!(
        lines[0].contains("Kept · protected by your whitelist"),
        "{out}"
    );
    assert!(lines[0].contains(file.to_str().unwrap()), "{out}");
    assert!(lines[1].ends_with("Eligible"), "{out}");
}

#[cfg(unix)]
#[test]
fn json_names_the_whitelist_file_or_default_as_the_source() {
    let home = fixture_home();
    let file = write_whitelist(home.path(), "~/Projects/keep\n");
    let text = stdout(explain(home.path()).args([
        "--json",
        "~/Projects/keep/a",
        "~/Library/Caches/kitty",
    ]));
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    let items = value["items"].as_array().unwrap();
    assert_eq!(items[0]["layer"], "whitelist");
    assert_eq!(items[0]["reason"], "whitelisted");
    assert_eq!(items[0]["source"], file.to_str().unwrap());
    assert_eq!(items[1]["layer"], "whitelist");
    assert_eq!(items[1]["source"], "default");
}

#[cfg(unix)]
#[test]
fn an_unreadable_whitelist_stops_the_command_and_names_the_file() {
    use std::os::unix::fs::PermissionsExt;
    let home = fixture_home();
    let file = write_whitelist(home.path(), "~/Projects/keep\n");
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000)).unwrap();
    explain(home.path())
        .arg("~/Projects/keep/a")
        .assert()
        .code(1)
        .stdout("")
        .stderr(predicate::str::contains("cannot read the whitelist"))
        .stderr(predicate::str::contains(file.to_str().unwrap()));
}

#[cfg(unix)]
#[test]
fn xdg_config_home_names_the_folder_the_whitelist_is_read_from() {
    let home = fixture_home();
    let xdg = fresh_dir();
    let dir = xdg.path().join("dustpan");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("whitelist"), "~/Projects/keep\n").unwrap();
    let out = stdout(
        explain(home.path())
            .env("XDG_CONFIG_HOME", xdg.path())
            .arg("~/Projects/keep/a"),
    );
    assert!(
        out.contains(dir.join("whitelist").to_str().unwrap()),
        "{out}"
    );
}

#[cfg(unix)]
const BLOCKED_READ_FAILS_AFTER: std::time::Duration = std::time::Duration::from_secs(20);

#[cfg(unix)]
fn stops_on_the_whitelist(home: &Path, cause: &str) {
    explain(home)
        .timeout(BLOCKED_READ_FAILS_AFTER)
        .arg("~/Projects/keep/a")
        .assert()
        .code(1)
        .stdout("")
        .stderr(predicate::str::contains("cannot read the whitelist"))
        .stderr(predicate::str::contains(cause));
}

#[cfg(unix)]
fn config_dir(home: &Path) -> std::path::PathBuf {
    let dir = home.join(".config").join("dustpan");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[cfg(unix)]
#[test]
fn a_whitelist_that_links_to_a_missing_file_stops_the_command() {
    let home = fixture_home();
    let link = config_dir(home.path()).join("whitelist");
    std::os::unix::fs::symlink(home.path().join("gone").join("whitelist"), &link).unwrap();
    stops_on_the_whitelist(home.path(), "does not exist");
}

#[cfg(unix)]
#[test]
fn a_config_folder_that_links_to_an_unmounted_volume_stops_the_command() {
    let home = fixture_home();
    let config = home.path().join(".config");
    std::os::unix::fs::symlink("/Volumes/dustpan-test-no-such-volume/config", &config).unwrap();
    stops_on_the_whitelist(home.path(), config.to_str().unwrap());
}

#[cfg(unix)]
#[test]
fn a_whitelist_that_is_a_fifo_stops_the_command() {
    let home = fixture_home();
    let fifo = config_dir(home.path()).join("whitelist");
    let made = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .unwrap();
    assert!(made.success());
    stops_on_the_whitelist(home.path(), "not a regular file");
}

#[cfg(unix)]
#[test]
fn a_whitelist_that_links_to_a_device_stops_the_command() {
    let home = fixture_home();
    let link = config_dir(home.path()).join("whitelist");
    std::os::unix::fs::symlink("/dev/zero", &link).unwrap();
    stops_on_the_whitelist(home.path(), "not a regular file");
}

#[cfg(unix)]
#[test]
fn a_whitelist_larger_than_a_hand_kept_list_stops_the_command() {
    let home = fixture_home();
    let body = "# padding line for the size cap test\n".repeat(64 * 1024);
    write_whitelist(home.path(), &body);
    stops_on_the_whitelist(home.path(), "larger than");
}

#[cfg(unix)]
#[test]
fn an_xdg_config_home_that_holds_dot_dot_is_named_as_the_cause() {
    let home = fixture_home();
    let xdg = fresh_dir();
    let spelled = xdg.path().join("..").join(xdg.path().file_name().unwrap());
    explain(home.path())
        .env("XDG_CONFIG_HOME", &spelled)
        .arg("~/Projects/keep/a")
        .assert()
        .code(1)
        .stderr(predicate::str::contains(spelled.to_str().unwrap()))
        .stderr(predicate::str::contains("`..`"));
}

#[cfg(unix)]
#[test]
fn an_entry_written_through_a_symlinked_folder_keeps_the_folder_behind_it() {
    let home = fixture_home();
    std::fs::create_dir_all(home.path().join("data/keep/x")).unwrap();
    std::os::unix::fs::symlink(home.path().join("data"), home.path().join("alias")).unwrap();
    let entry = home.path().join("alias/keep");
    write_whitelist(home.path(), &format!("{}\n", entry.to_str().unwrap()));
    let out = stdout(explain(home.path()).args(["~/data/keep/x", "~/data"]));
    let lines: Vec<&str> = out.lines().collect();
    assert!(lines[0].contains("protected by your whitelist"), "{out}");
    assert!(lines[1].contains("protected by your whitelist"), "{out}");
}

#[cfg(target_os = "macos")]
#[test]
fn an_entry_keeps_a_folder_stored_in_the_other_normalization_form() {
    let home = fixture_home();
    std::fs::create_dir_all(home.path().join("Docs/Cafe\u{301}/sub")).unwrap();
    write_whitelist(home.path(), "~/Docs/Caf\u{e9}\n");
    let out = stdout(explain(home.path()).arg("~/Docs/Cafe\u{301}/sub"));
    assert!(out.contains("protected by your whitelist"), "{out}");
}
