//! Who keeps a cache, judged by the release path: real processes, the real process table, the
//! real open files and `explain`. A process keeps the folder it holds a file open in, whatever
//! the folder is named. A running app keeps its group container's caches, and the folder names
//! of a bundle make no process an owner (SAFE-08, SAFE-09).
#![cfg(target_os = "macos")]

mod support;

use std::io::{BufRead as _, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use support::Fixture;

#[test]
fn helper_sleeps_when_asked() {
    support::sleep_if_asked();
}

const HOLD: &str = "DUSTPAN_TEST_HOLD";

/// What this binary runs as a child of the test below: it opens the file, says `held`, closes
/// the file when its input ends, says `closed`, and stays alive.
#[test]
fn helper_holds_a_file_when_asked() {
    let Some(path) = std::env::var_os(HOLD) else {
        return;
    };
    let held = std::fs::File::open(path).unwrap();
    println!("held");
    std::io::copy(&mut std::io::stdin(), &mut std::io::sink()).unwrap();
    drop(held);
    println!("closed");
    std::thread::sleep(std::time::Duration::from_secs(120));
}

fn wait_for(lines: &mut impl Iterator<Item = std::io::Result<String>>, word: &str) {
    assert!(
        lines.any(|line| line.unwrap() == word),
        "the helper never said {word}"
    );
}

#[test]
fn a_process_keeps_the_folder_it_holds_a_file_open_in() {
    let fixture = Fixture::new();
    let vendor = fixture.home_dir("Library/Caches/BraveSoftware");
    let profile = fixture.home_dir("Library/Caches/BraveSoftware/Brave-Browser");
    let (db, wal) = (
        format!("{profile}/state.db"),
        format!("{profile}/state.db-wal"),
    );
    std::fs::write(&db, "x").unwrap();
    std::fs::write(&wal, "x").unwrap();

    #[expect(
        clippy::disallowed_methods,
        reason = "the holder is this test binary, run again as a helper"
    )]
    let mut holder = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "helper_holds_a_file_when_asked", "--nocapture"])
        .env(HOLD, &db)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut said = BufReader::new(holder.stdout.take().unwrap()).lines();
    wait_for(&mut said, "held");
    let kept = format!(
        "Kept · process {} has {db} open; quit it first",
        holder.id()
    );
    // The folder is named for a vendor: no bundle id, app group or process name leads to it.
    let while_open = [&vendor, &profile, &db, &wal].map(|path| fixture.explain(path));

    drop(holder.stdin.take());
    wait_for(&mut said, "closed");
    let once_closed = [&vendor, &profile, &db, &wal].map(|path| fixture.explain(path));
    holder.kill().unwrap();
    holder.wait().unwrap();

    assert_eq!(while_open, [(); 4].map(|()| kept.clone()));
    assert_eq!(once_closed, [(); 4].map(|()| "Eligible".to_owned()));
}

/// Writes an `Info.plist` that names `bundle_id` into the folder `contents` of the fixture.
fn bundle(fixture: &Fixture, contents: &str, bundle_id: &str) {
    let contents = fixture.root.join(contents);
    std::fs::create_dir_all(&contents).unwrap();
    std::fs::write(contents.join("Info.plist"), support::plist(bundle_id)).unwrap();
}

#[test]
fn a_running_app_keeps_the_caches_of_its_group_containers() {
    let fixture = Fixture::new();
    bundle(&fixture, "Widget.app/Contents", "com.example.widgetapp");
    let app = fixture.run("Widget.app/Contents/MacOS/widget");
    for group in [
        "ABCDE12345.com.example.widgetapp",
        "group.com.example.widgetapp",
    ] {
        let caches = fixture.home_dir(&format!("Library/Group Containers/{group}/Library/Caches"));
        assert_eq!(
            fixture.explain(&caches),
            format!(
                "Kept · a process of the app group {group} is running (pid {}); quit it first",
                app.pid()
            )
        );
    }
    let other = fixture.home_dir("Library/Group Containers/group.com.example.other/Library/Caches");
    assert_eq!(fixture.explain(&other), "Eligible");
}

/// Signs `exe` ad hoc with a signing identifier and an entitlement to `app_groups`.
fn sign(exe: &Path, identifier: &str, app_groups: &[&str]) {
    let groups: String = app_groups
        .iter()
        .map(|group| format!("<string>{group}</string>"))
        .collect();
    let entitlements = exe.with_extension("entitlements");
    let text = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>com.apple.security.application-groups</key><array>{groups}</array>
</dict></plist>
"#
    );
    std::fs::write(&entitlements, text).unwrap();
    #[expect(
        clippy::disallowed_methods,
        reason = "codesign signs the fixture app with its entitlements"
    )]
    let signed = Command::new("codesign")
        .args(["--force", "--sign", "-", "--identifier", identifier])
        .arg("--entitlements")
        .args([&entitlements, exe])
        .output()
        .unwrap();
    assert!(signed.status.success(), "{signed:?}");
}

#[test]
fn a_process_keeps_the_caches_of_the_groups_its_entitlements_name() {
    let fixture = Fixture::new();
    let exe = fixture.place("tools/syncd");
    sign(
        &exe,
        "com.dustpan.test.syncd",
        &["group.com.dustpan.test.shared", "ABCDE12345.dustpan-test"],
    );
    let daemon = fixture.start(&exe);
    for group in ["group.com.dustpan.test.shared", "ABCDE12345.dustpan-test"] {
        let caches = fixture.home_dir(&format!("Library/Group Containers/{group}/Caches"));
        assert_eq!(
            fixture.explain(&caches),
            format!(
                "Kept · a process of the app group {group} is running (pid {}); quit it first",
                daemon.pid()
            )
        );
    }
    // The daemon sits in no bundle: its signature alone names it.
    let cache = fixture.home_dir("Library/Caches/com.dustpan.test.syncd");
    assert_eq!(
        fixture.explain(&cache),
        format!(
            "Kept · com.dustpan.test.syncd is running (pid {}); quit it first",
            daemon.pid()
        )
    );
}

#[test]
fn the_folder_names_of_a_bundle_make_no_process_an_owner() {
    let fixture = Fixture::new();
    bundle(&fixture, "Other.app/Contents", "com.dustpan.test.other");
    let _bundled = fixture.run("Other.app/Contents/MacOS/helper");
    let _bare = fixture.run("tools/Contents/MacOS/helper");
    let cache = fixture.home_dir("Library/Caches/com.example.macos.helper");
    assert_eq!(fixture.explain(&cache), "Eligible");
}
