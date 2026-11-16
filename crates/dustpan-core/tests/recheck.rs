//! `Guard::recheck` is the only way to an `Approved` (SAFE-10).
#![cfg(unix)]

use dustpan_core::candidate::{Candidate, GroupId, Rule, RuleId, Sink};
use dustpan_core::guard::{
    Guard, GuardInputs, IdentityReason, KeptReason, Layer, LiveReason, Rejection, ShapeReason,
    Verdict,
};
use dustpan_core::identity::Node;
use dustpan_core::path::AbsPath;
use dustpan_core::paths::Os;
use dustpan_core::probe::fake::FakeProbe;
use dustpan_core::probe::{Owner, Probe, Seen, Unseen};
use dustpan_core::whitelist::Whitelist;
use std::sync::Arc;

const APP: &str = "/Users/alice/Library/Caches/com.example.app";
const CACHE: &str = "/Users/alice/Library/Caches/com.example.app/blob";

fn p(s: &str) -> AbsPath {
    AbsPath::parse(s.as_ref()).unwrap()
}

fn fixture() -> FakeProbe {
    FakeProbe::new().dir("/Users/alice/Documents").dir(CACHE)
}

fn rule() -> Arc<Rule> {
    Arc::new(Rule {
        id: RuleId("test.cache".into()),
        group: GroupId("test".into()),
        owners: Box::new([Owner::BundleId("com.example.app".into())]),
        sink: Sink::Delete,
        default_selected: true,
    })
}

/// What a scanner's directory read would report for the entry.
fn seen(fs: &FakeProbe, path: &str) -> Node {
    match fs.node(&p(path)) {
        Seen::Found(node) => node,
        other => panic!("fixture {path} is {other:?}"),
    }
}

fn guard(fs: &FakeProbe) -> Guard {
    let env = |_: &str| None;
    Guard::new(
        GuardInputs {
            os: Os::MacOs,
            home: p("/Users/alice"),
            env: &env,
            whitelist: Whitelist::defaults(Os::MacOs, &p("/Users/alice")),
        },
        fs,
    )
    .unwrap()
}

fn discover(guard: &Guard, fs: &FakeProbe) -> Candidate {
    let candidate = guard
        .pass(fs)
        .candidate(p(CACHE), seen(fs, CACHE), &rule())
        .unwrap();
    assert_eq!(candidate.verdict(), &Verdict::Eligible);
    candidate
}

fn refusal(guard: &Guard, fs: &FakeProbe, candidate: Candidate) -> Rejection {
    guard.recheck(candidate, fs).unwrap_err().rejection
}

#[test]
fn an_untouched_candidate_is_approved_with_discovery_identity() {
    let fs = fixture();
    let guard = guard(&fs);
    let candidate = discover(&guard, &fs);
    let expected = candidate.identity();
    let approved = guard.recheck(candidate, &fs).unwrap();
    assert_eq!(approved.identity(), expected);
    assert_eq!(approved.path(), &p(CACHE));
    assert!(matches!(approved.sink(), Sink::Delete));
}

#[test]
fn a_file_opened_after_discovery_is_refused_at_the_sink() {
    let fs = fixture();
    let guard = guard(&fs);
    let candidate = discover(&guard, &fs);
    let held = format!("{CACHE}/data_0");
    fs.open_now(31, &held);
    let rejection = refusal(&guard, &fs, candidate);
    assert!(
        matches!(&rejection, Rejection::Kept(KeptReason::Live(LiveReason::InUse { pid, file }))
            if pid.0 == 31 && *file == p(&held)),
        "{rejection:?}"
    );

    fs.close_all();
    let candidate = discover(&guard, &fs);
    let snapshots = fs.open_file_calls();
    guard.recheck(candidate, &fs).unwrap();
    assert_eq!(fs.open_file_calls(), snapshots + 1);
}

#[test]
fn a_process_whose_open_files_cannot_be_listed_at_the_sink_is_a_refusal() {
    let fs = fixture();
    let guard = guard(&fs);
    let candidate = discover(&guard, &fs);
    fs.unlisted_now(31, true);
    let rejection = refusal(&guard, &fs, candidate);
    assert!(
        matches!(
            &rejection,
            Rejection::Kept(KeptReason::Live(LiveReason::InUseUnknown(Unseen::Unlisted(pid))))
                if pid.0 == 31
        ),
        "{rejection:?}"
    );

    // Another user's process holds nothing in this user's cache.
    let fs = fixture();
    let candidate = discover(&guard, &fs);
    fs.unlisted_now(1, false);
    guard.recheck(candidate, &fs).unwrap();
}

#[test]
fn a_swapped_inode_is_refused() {
    let fs = fixture();
    let guard = guard(&fs);
    let candidate = discover(&guard, &fs);
    fs.swap(CACHE);
    let rejection = refusal(&guard, &fs, candidate);
    assert!(
        matches!(&rejection, Rejection::Kept(reason @ KeptReason::Identity(IdentityReason::Changed))
            if reason.layer() == Layer::Identity && reason.code() == "identity-changed"),
        "{rejection:?}"
    );
}

#[test]
fn a_swapped_parent_is_refused() {
    let fs = fixture();
    let guard = guard(&fs);
    let candidate = discover(&guard, &fs);
    fs.swap(APP);
    assert!(matches!(
        refusal(&guard, &fs, candidate),
        Rejection::Kept(KeptReason::Identity(IdentityReason::Changed))
    ));
}

#[test]
fn a_parent_replaced_by_a_symlink_is_refused() {
    let fs = fixture();
    let guard = guard(&fs);
    let candidate = discover(&guard, &fs);
    fs.replace_with_symlink(APP, "/Users/alice/Documents");
    let rejection = refusal(&guard, &fs, candidate);
    assert!(
        matches!(&rejection, Rejection::Kept(KeptReason::Shape(ShapeReason::LinkedAncestor { link }))
            if *link == p(APP)),
        "{rejection:?}"
    );
}

#[test]
fn a_target_swapped_during_the_owner_probe_is_refused() {
    let fs = fixture();
    let guard = guard(&fs);
    let candidate = discover(&guard, &fs);
    fs.swap_during_next_process_probe(CACHE);
    let rejection = refusal(&guard, &fs, candidate);
    assert!(
        matches!(&rejection, Rejection::Kept(reason @ KeptReason::Identity(IdentityReason::ReplacedDuringCheck))
            if reason.layer() == Layer::Identity && reason.code() == "replaced-during-check"),
        "{rejection:?}"
    );
}

#[test]
fn a_path_that_is_gone_is_reported_as_gone_not_as_kept() {
    let fs = fixture();
    let guard = guard(&fs);
    let candidate = discover(&guard, &fs);
    fs.remove(CACHE);
    assert!(matches!(refusal(&guard, &fs, candidate), Rejection::Gone));
}

#[test]
fn an_alias_the_scan_could_not_tell_apart_is_refused_at_the_sink() {
    // An all-ASCII name that the file system stores under a kept spelling. A scan trusts its
    // directory read for such a name; `recheck` asks the file system.
    let keychain = "/Users/alice/Library/Keychains/login.keychain-db";
    let alias = "/Users/alice/Library/Caches/com.example.app/alias";
    let fs = fixture().file(keychain).alias(alias, keychain);
    let guard = guard(&fs);
    let candidate = guard
        .pass(&fs)
        .candidate(p(alias), seen(&fs, alias), &rule())
        .unwrap();
    assert_eq!(candidate.verdict(), &Verdict::Eligible);
    let rejection = refusal(&guard, &fs, candidate);
    assert!(
        matches!(&rejection, Rejection::Kept(reason @ KeptReason::Shape(ShapeReason::AliasOfKept { stored, .. }))
            if *stored == p(keychain) && reason.code() == "alias-of-kept"),
        "{rejection:?}"
    );
}

#[test]
fn an_owner_that_started_after_discovery_is_refused() {
    let fs = fixture();
    let guard = guard(&fs);
    let candidate = discover(&guard, &fs);
    let now = fixture().running(
        9,
        "/Applications/Example.app/Contents/MacOS/com.example.app",
    );
    // Same tree, so the identities match; only the process table differs.
    let rejection = guard.recheck(candidate, &now).unwrap_err().rejection;
    assert!(
        matches!(
            rejection,
            Rejection::Kept(KeptReason::Live(LiveReason::OwnerRunning { .. }))
        ),
        "{rejection:?}"
    );
}

#[test]
fn recheck_takes_its_own_process_table() {
    let fs = fixture();
    let guard = guard(&fs);
    let candidate = discover(&guard, &fs);
    let before = fs.process_calls();
    let _ = guard.recheck(candidate, &fs);
    assert_eq!(fs.process_calls(), before + 1);
}

#[test]
fn the_refused_candidate_comes_back_for_the_outcome_row() {
    let fs = fixture();
    let guard = guard(&fs);
    let candidate = discover(&guard, &fs);
    fs.swap(CACHE);
    let refused = guard.recheck(candidate, &fs).unwrap_err();
    assert_eq!(refused.candidate.path(), &p(CACHE));
    assert_eq!(refused.candidate.rule().id, RuleId("test.cache".into()));
}
