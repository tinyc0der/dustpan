//! An `Info.plist` that is not a small regular file never stalls the guard: the process next
//! to it is unidentified, which keeps every owned cache (SAFE-08), and `explain` still answers.
#![cfg(target_os = "macos")]

mod support;

use std::path::Path;
use std::process::Command;
use std::sync::{Mutex, PoisonError};
use support::Fixture;

#[test]
fn helper_sleeps_when_asked() {
    support::sleep_if_asked();
}

/// Each case leaves an unidentified process behind while it runs, and the refusal names the
/// process, so a case must not see another case's process.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

fn explain_names_the_process_beside(lay: fn(&Path)) {
    let _alone = ONE_AT_A_TIME.lock().unwrap_or_else(PoisonError::into_inner);
    let fixture = Fixture::new();
    let cache = fixture.home_dir("Library/Caches/com.example.widget");
    let process = fixture.run("x.d/sl");
    lay(&fixture.root.join("x.d/Info.plist"));
    assert_eq!(
        fixture.explain(&cache),
        format!(
            "Kept · cannot tell whether com.example.widget is running: \
             process {} cannot be identified; quit it first",
            process.pid()
        )
    );
}

/// The process runs below more than a hundred folders named like bundles, each with a
/// well-formed megabyte that names no id.
#[test]
fn a_deep_tree_of_large_info_plists_does_not_slow_explain() {
    let _alone = ONE_AT_A_TIME.lock().unwrap_or_else(PoisonError::into_inner);
    let fixture = Fixture::new();
    let cache = fixture.home_dir("Library/Caches/com.example.widget");

    let (head, tail) = (
        "<plist version=\"1.0\"><dict><key>k</key><array>",
        "</array></dict></plist>",
    );
    let item = "<real>1.5e3</real>";
    let mut text = format!(
        "{head}{}{tail}",
        item.repeat(((1 << 20) - head.len() - tail.len()) / item.len())
    );
    text.push_str(&" ".repeat((1 << 20) - text.len()));
    let big = fixture.root.join("big.plist");
    std::fs::write(&big, text).unwrap();

    let mut dir = fixture.root.join("deep");
    let mut levels = 0;
    while dir.as_os_str().len() + "/a.b/Info.plist".len() < 1000 {
        dir.push("a.b");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::hard_link(&big, dir.join("Info.plist")).unwrap();
        levels += 1;
    }
    assert!(levels > 100, "only {levels} levels fit below the fixture");

    let exe = dir.join("sl");
    std::fs::copy(std::env::current_exe().unwrap(), &exe).unwrap();
    let process = fixture.start(&exe);
    assert_eq!(
        fixture.explain(&cache),
        format!(
            "Kept · cannot tell whether com.example.widget is running: \
             process {} cannot be identified; quit it first",
            process.pid()
        )
    );
}

#[test]
fn a_fifo_named_info_plist_does_not_hang_explain() {
    explain_names_the_process_beside(|plist| {
        let made = Command::new("mkfifo").arg(plist).status().unwrap();
        assert!(made.success());
    });
}

#[test]
fn an_info_plist_that_links_to_an_endless_device_is_not_read() {
    explain_names_the_process_beside(|plist| {
        std::os::unix::fs::symlink("/dev/zero", plist).unwrap();
    });
}

#[test]
fn an_info_plist_over_the_size_cap_is_not_trusted() {
    explain_names_the_process_beside(|plist| {
        let mut text = support::plist("com.dustpan.test.padded");
        text.push_str(&" ".repeat((1 << 20) + 1 - text.len()));
        std::fs::write(plist, text).unwrap();
    });
}
