//! Who owns a cache, judged by the release path: real processes, the real process table and
//! `explain`. A running app keeps its group container's caches, and the folder names of a
//! bundle make no process an owner (SAFE-08).
#![cfg(target_os = "macos")]

mod support;

use std::path::Path;
use std::process::Command;
use support::Fixture;

#[test]
fn helper_sleeps_when_asked() {
    support::sleep_if_asked();
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
