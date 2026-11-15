//! Who owns a cache, judged by the release path: real processes, the real process table and
//! `explain`. A running app keeps its group container's caches, and the folder names of a
//! bundle make no process an owner (SAFE-08).
#![cfg(target_os = "macos")]

mod support;

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
                "Kept · an app of the group {group} is running (pid {}); quit it first",
                app.pid()
            )
        );
    }
    let other = fixture.home_dir("Library/Group Containers/group.com.example.other/Library/Caches");
    assert_eq!(fixture.explain(&other), "Eligible");
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
