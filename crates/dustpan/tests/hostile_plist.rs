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
