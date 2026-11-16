//! `Recheck::approve` is the only way to an `Approved` (SAFE-10).
#![cfg(unix)]

use dustpan_core::candidate::{Candidate, GroupId, Rule, RuleId, Sink};
use dustpan_core::guard::{
    Approved, Bound, ContentsReason, Guard, GuardInputs, Homes, IdentityReason, KeptReason, Layer,
    Lease, LiveReason, Refused, Rejection, ShapeReason, Verdict,
};
use dustpan_core::identity::Node;
use dustpan_core::path::AbsPath;
use dustpan_core::paths::Os;
use dustpan_core::probe::fake::FakeProbe;
use dustpan_core::probe::{Owner, Probe, Seen, Unseen};
use dustpan_core::time::{FixedClock, WallTime};
use dustpan_core::whitelist::Whitelist;
use std::sync::Arc;
use std::time::Duration;

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
            homes: Homes::new(p("/Users/alice"), None),
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

fn clock() -> FixedClock {
    FixedClock::new(WallTime {
        unix_secs: 1_763_280_000,
        utc_offset_secs: 0,
    })
}

/// One recheck with its own evidence, as the privileged helper runs it.
#[expect(
    clippy::result_large_err,
    reason = "mirrors Recheck::approve, which hands the candidate back"
)]
fn approve(guard: &Guard, fs: &FakeProbe, candidate: Candidate) -> Result<Approved, Refused> {
    guard.recheck(fs, &clock(), Lease::NONE).approve(candidate)
}

fn refusal(guard: &Guard, fs: &FakeProbe, candidate: Candidate) -> Rejection {
    approve(guard, fs, candidate).unwrap_err().rejection
}

#[test]
fn an_untouched_candidate_is_approved_with_discovery_identity() {
    let fs = fixture();
    let guard = guard(&fs);
    let candidate = discover(&guard, &fs);
    let expected = candidate.identity();
    let approved = approve(&guard, &fs, candidate).unwrap();
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
    approve(&guard, &fs, candidate).unwrap();
    assert_eq!(
        fs.open_file_calls(),
        snapshots + 2,
        "one for the folder, one after its survey"
    );
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
    approve(&guard, &fs, candidate).unwrap();
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
    let rejection = approve(&guard, &now, candidate).unwrap_err().rejection;
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
    let _ = approve(&guard, &fs, candidate);
    assert_eq!(
        fs.process_calls(),
        before + 2,
        "one for the folder, one after its survey"
    );
}

#[test]
fn the_refused_candidate_comes_back_for_the_outcome_row() {
    let fs = fixture();
    let guard = guard(&fs);
    let candidate = discover(&guard, &fs);
    fs.swap(CACHE);
    let refused = approve(&guard, &fs, candidate).unwrap_err();
    assert_eq!(refused.candidate.path(), &p(CACHE));
    assert_eq!(refused.candidate.rule().id, RuleId("test.cache".into()));
}

fn discover_at(guard: &Guard, fs: &FakeProbe, path: &str) -> Candidate {
    guard
        .pass(fs)
        .candidate(p(path), seen(fs, path), &rule())
        .unwrap()
}

#[test]
fn a_batch_shares_live_evidence_until_it_is_older_than_the_lease() {
    let (a, b) = (format!("{APP}/a"), format!("{APP}/b"));
    let fs = fixture().dir(&a).dir(&b);
    let guard = guard(&fs);
    let plan = [CACHE, &a, &b].map(|path| discover_at(&guard, &fs, path));
    let clock = clock();
    let mut batch = guard.recheck(&fs, &clock, Lease::BATCH);
    let calls = || (fs.process_calls(), fs.open_file_calls());
    let [first, second, third] = plan;
    let before = calls();
    batch.approve(first).unwrap();
    assert_eq!(
        calls(),
        (before.0 + 1, before.1 + 1),
        "the first check takes both"
    );
    clock.advance(Duration::from_millis(999));
    batch.approve(second).unwrap();
    assert_eq!(
        calls(),
        (before.0 + 1, before.1 + 1),
        "inside the lease nothing is taken"
    );
    clock.advance(Duration::from_millis(1));
    batch.approve(third).unwrap();
    assert_eq!(
        calls(),
        (before.0 + 2, before.1 + 2),
        "at the lease's end both again"
    );
}

#[test]
fn no_lease_takes_fresh_evidence_for_every_candidate() {
    let a = format!("{APP}/a");
    let fs = fixture().dir(&a);
    let guard = guard(&fs);
    let [first, second] = [CACHE, &a].map(|path| discover_at(&guard, &fs, path));
    let clock = clock();
    let mut batch = guard.recheck(&fs, &clock, Lease::NONE);
    let before = (fs.process_calls(), fs.open_file_calls());
    batch.approve(first).unwrap();
    batch.approve(second).unwrap();
    assert_eq!(
        (fs.process_calls(), fs.open_file_calls()),
        (before.0 + 4, before.1 + 4),
        "each folder takes both before and after its survey"
    );
}

#[test]
fn a_file_opened_inside_the_lease_is_seen_once_the_lease_ends() {
    let a = format!("{APP}/a");
    let fs = fixture().dir(&a);
    let guard = guard(&fs);
    let clock = clock();
    let mut batch = guard.recheck(&fs, &clock, Lease::BATCH);
    batch.approve(discover_at(&guard, &fs, CACHE)).unwrap();
    let candidate = discover_at(&guard, &fs, &a);
    fs.open_now(7, &a);
    clock.advance(Lease::BATCH.window());
    let refused = batch.approve(candidate).unwrap_err();
    assert!(
        matches!(
            refused.rejection,
            Rejection::Kept(KeptReason::Live(LiveReason::InUse { .. }))
        ),
        "{:?}",
        refused.rejection
    );
}

fn contents(fs: &FakeProbe) -> ContentsReason {
    let guard = guard(fs);
    let candidate = discover(&guard, fs);
    match refusal(&guard, fs, candidate) {
        Rejection::Kept(KeptReason::Contents(reason)) => reason,
        other => panic!("expected a contents refusal, got {other:?}"),
    }
}

fn holds(fs: &FakeProbe) -> (AbsPath, KeptReason) {
    match contents(fs) {
        ContentsReason::Holds { at, reason } => (at, *reason),
        other => panic!("expected holds-kept, got {other:?}"),
    }
}

fn approves(fs: &FakeProbe) {
    let guard = guard(fs);
    let candidate = discover(&guard, fs);
    if let Err(refused) = approve(&guard, fs, candidate) {
        panic!("refused: {:?}", refused.rejection);
    }
}

#[test]
fn a_mount_inside_a_folder_keeps_the_folder() {
    let inner = format!("{CACHE}/x/mnt");
    let fs = fixture().mount(&inner, 2).file(&format!("{inner}/kept"));
    assert_eq!(
        holds(&fs),
        (p(&inner), KeptReason::Shape(ShapeReason::MountPoint))
    );
}

#[test]
fn a_bind_mount_on_the_same_volume_keeps_the_folder() {
    let inner = format!("{CACHE}/bound");
    let fs = fixture().bind_mount(&inner);
    assert_eq!(
        holds(&fs),
        (p(&inner), KeptReason::Shape(ShapeReason::MountPoint))
    );
}

#[test]
fn a_protected_row_deep_inside_keeps_the_folder() {
    let inner = format!("{CACHE}/x/.claude/projects");
    let fs = fixture().file(&format!("{inner}/session.jsonl"));
    let (at, reason) = holds(&fs);
    assert_eq!(at, p(&inner));
    assert_eq!(reason.layer(), Layer::Protection, "{reason:?}");
}

/// Bundle-id names mark an app's own folder. Inside a cache folder the guard approved, a folder
/// named for an app is part of that cache.
#[test]
fn a_folder_named_for_an_app_inside_is_part_of_the_cache() {
    approves(&fixture().file(&format!("{CACHE}/x/com.apple.Safari/state")));
    approves(&fixture().file(&format!("{CACHE}/io.tailscale.ipn/deep/state")));
}

#[test]
fn an_unreadable_folder_inside_keeps_the_folder() {
    let fs = fixture().unreadable(&format!("{CACHE}/locked"));
    let (_, reason) = holds(&fs);
    assert_eq!(reason.code(), "unreadable", "{reason:?}");
}

#[test]
fn an_online_only_file_inside_keeps_the_folder() {
    let ghost = format!("{CACHE}/cloud.bin");
    let fs = fixture().dataless(&ghost);
    assert_eq!(contents(&fs), ContentsReason::Dataless { at: p(&ghost) });
}

#[test]
fn a_tree_deeper_than_the_walk_can_hold_is_refused_up_front() {
    let nested =
        |levels: usize| (0..levels).fold(CACHE.to_owned(), |path, i| format!("{path}/{i}"));
    assert_eq!(
        contents(&fixture().dir(&nested(65))),
        ContentsReason::Unbounded(Bound::Depth(64))
    );
    approves(&fixture().dir(&nested(64)));
}

#[test]
fn a_name_with_a_control_byte_is_judged_not_refused() {
    approves(&fixture().file_in(CACHE, "bad\nname"));
}

#[test]
fn a_link_inside_is_judged_as_a_link_not_by_its_target() {
    approves(&fixture().symlink(&format!("{CACHE}/docs"), "/Users/alice/Documents"));
}

/// Approves `at` in a batch on `clock`, after `arm` has set what happens during the survey.
#[expect(
    clippy::result_large_err,
    reason = "mirrors Recheck::approve, which hands the candidate back"
)]
fn approve_during(
    fs: &FakeProbe,
    at: &str,
    lease: Lease,
    arm: impl FnOnce(&FakeProbe, Arc<FixedClock>),
) -> Result<Approved, Refused> {
    let guard = guard(fs);
    let candidate = discover_at(&guard, fs, at);
    let clock = Arc::new(clock());
    arm(fs, Arc::clone(&clock));
    guard.recheck(fs, &*clock, lease).approve(candidate)
}

#[test]
fn a_survey_that_outlasts_its_deadline_in_its_last_listing_is_refused() {
    let fs = fixture()
        .file(&format!("{CACHE}/one"))
        .file(&format!("{CACHE}/two"));
    let refused = approve_during(&fs, CACHE, Lease::NONE, |fs, clock| {
        fs.after_listing(CACHE, move |_| clock.advance(Duration::from_secs(31)));
    })
    .unwrap_err();
    assert!(
        matches!(
            refused.rejection,
            Rejection::Kept(KeptReason::Contents(ContentsReason::Unbounded(
                Bound::Deadline(_)
            )))
        ),
        "{:?}",
        refused.rejection
    );
}

#[test]
fn a_file_opened_during_the_survey_keeps_the_folder() {
    let held = format!("{CACHE}/one");
    let fs = fixture().file(&held);
    let opened = held.clone();
    let refused = approve_during(&fs, CACHE, Lease::NONE, move |fs, _| {
        fs.after_listing(CACHE, move |fs| fs.open_now(9, &opened));
    })
    .unwrap_err();
    assert!(
        matches!(&refused.rejection, Rejection::Kept(KeptReason::Live(LiveReason::InUse { pid, file }))
            if pid.0 == 9 && *file == p(&held)),
        "{:?}",
        refused.rejection
    );
}

/// A batch keeps its evidence while it is younger than the lease, even across a survey; once
/// the survey has aged it past the lease, the candidate is judged again on new evidence.
#[test]
fn a_survey_longer_than_the_lease_takes_the_evidence_again() {
    let held = format!("{CACHE}/one");
    let open_after = |by: Duration| {
        let fs = fixture().file(&held);
        let opened = held.clone();
        approve_during(&fs, CACHE, Lease::BATCH, move |fs, clock| {
            fs.after_listing(CACHE, move |fs| {
                fs.open_now(9, &opened);
                clock.advance(by);
            });
        })
    };
    assert!(open_after(Duration::from_millis(500)).is_ok());
    let refused = open_after(Duration::from_millis(1500)).unwrap_err();
    assert!(
        matches!(
            refused.rejection,
            Rejection::Kept(KeptReason::Live(LiveReason::InUse { .. }))
        ),
        "{:?}",
        refused.rejection
    );
}

/// A whole container holds a cache scope its app names. The app starting while the survey
/// runs keeps the container, though the container itself names no owner.
#[test]
fn an_app_that_starts_during_the_survey_keeps_the_container_whose_caches_it_owns() {
    const CONTAINER: &str = "/Users/alice/Library/Containers/com.example.other";
    let fs = fixture().file(&format!("{CONTAINER}/Data/Library/Caches/blob"));
    let refused = approve_during(&fs, CONTAINER, Lease::NONE, |fs, _| {
        fs.after_listing(CONTAINER, |fs| {
            fs.app_starts_now(
                44,
                "/Applications/Other.app/Contents/MacOS/Other",
                "com.example.other",
            );
        });
    })
    .unwrap_err();
    assert!(
        matches!(&refused.rejection,
            Rejection::Kept(KeptReason::Contents(ContentsReason::Holds { reason, .. }))
                if matches!(**reason, KeptReason::Live(LiveReason::OwnerRunning { .. }))),
        "{:?}",
        refused.rejection
    );
}
