//! Spellings the kernel resolves to a kept file, judged with the real probe on the real file
//! system. Which names are one file is the volume's decision, so the in-memory fake cannot
//! stand in for it. The tests live here because `dustpan-core` targets make no OS call.
#![cfg(target_os = "macos")]

use dustpan_core::candidate::{GroupId, Rule, RuleId, Sink};
use dustpan_core::guard::{Guard, GuardInputs, Homes, Lease, Verdict};
use dustpan_core::identity::Node;
use dustpan_core::path::AbsPath;
use dustpan_core::paths::Os;
use dustpan_core::probe::{Probe, Seen};
use dustpan_core::whitelist::Whitelist;
use dustpan_sys::LiveProbe;
use std::os::unix::fs::MetadataExt as _;
use std::sync::Arc;

fn p(s: &str) -> AbsPath {
    AbsPath::parse(s.as_ref()).unwrap()
}

/// A home folder with a keychain file and the cache of a protected bundle.
struct Home {
    dir: tempfile::TempDir,
    root: String,
}

impl Home {
    fn new() -> Self {
        let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
        let root = dir.path().canonicalize().unwrap();
        std::fs::create_dir_all(root.join("Library/Keychains")).unwrap();
        std::fs::write(root.join("Library/Keychains/login.keychain-db"), b"secret").unwrap();
        std::fs::create_dir_all(root.join("Library/Caches/com.microsoft.VSCode")).unwrap();
        let root = root.to_str().unwrap().to_owned();
        Self { dir, root }
    }

    fn at(&self, rest: &str) -> String {
        format!("{}/{rest}", self.root)
    }

    fn guard(&self, probe: &LiveProbe) -> Guard {
        let env = |_: &str| None;
        let inputs = GuardInputs {
            os: Os::MacOs,
            homes: Homes::new(p(&self.root), None),
            env: &env,
            whitelist: Whitelist::defaults(Os::MacOs, &Homes::new(p(&self.root), None)),
        };
        Guard::new(inputs, probe).unwrap()
    }
}

fn node(probe: &LiveProbe, path: &str) -> Node {
    match probe.node(&p(path)) {
        Seen::Found(node) => node,
        other => panic!("{path} is {other:?}"),
    }
}

fn delete_rule() -> Arc<Rule> {
    Arc::new(Rule {
        id: RuleId("test.any".into()),
        group: GroupId("test".into()),
        owners: Box::new([]),
        sink: Sink::Delete,
        default_selected: true,
    })
}

/// Kept by `evaluate` and by `candidate`, and never approved by `recheck`.
fn assert_kept_everywhere(guard: &Guard, probe: &LiveProbe, path: &str) {
    let seen = node(probe, path);
    let verdict = guard.pass(probe).evaluate(p(path)).verdict().clone();
    assert!(
        matches!(verdict, Verdict::Kept(_)),
        "evaluate {path}: {verdict:?}"
    );
    match guard.pass(probe).candidate(p(path), seen, &delete_rule()) {
        Ok(candidate) => {
            let verdict = candidate.verdict().clone();
            assert!(
                matches!(verdict, Verdict::Kept(_)),
                "candidate {path}: {verdict:?}"
            );
            let clock = dustpan_sys::SystemClock::new();
            let rechecked = guard.recheck(probe, &clock, Lease::NONE).approve(candidate);
            assert!(rechecked.is_err(), "recheck approved {path}");
        }
        // No candidate exists, so nothing can reach `recheck`.
        Err(judgment) => assert!(
            matches!(judgment.verdict(), Verdict::Kept(_)),
            "candidate {path}: {judgment:?}"
        ),
    }
}

#[test]
fn a_case_fold_spelling_of_a_kept_file_is_kept_and_never_approved() {
    let home = Home::new();
    let probe = LiveProbe::new();
    let guard = home.guard(&probe);
    let plain = home.at("Library/Keychains/login.keychain-db");
    for alias in [
        "Library/\u{212A}eychains/login.keychain-db",
        "Library/Keychain\u{17F}/login.keychain-db",
        "LIBRARY/KEYCHAINS/LOGIN.KEYCHAIN-DB",
        "Library/Keychains/login.\u{212A}eychain-db",
    ] {
        let alias = home.at(alias);
        assert_eq!(
            node(&probe, &alias).id,
            node(&probe, &plain).id,
            "{alias} is another file: {} is on a case-sensitive volume",
            home.dir.path().display()
        );
        assert_kept_everywhere(&guard, &probe, &alias);
    }
    assert_kept_everywhere(&guard, &probe, &plain);
}

#[test]
fn an_uppercase_spelling_of_a_protected_bundle_cache_is_kept() {
    let home = Home::new();
    let probe = LiveProbe::new();
    let guard = home.guard(&probe);
    for name in ["COM.MICROSOFT.VSCODE", "com.microsoft.VSCode"] {
        assert_kept_everywhere(&guard, &probe, &home.at(&format!("Library/Caches/{name}")));
    }
}

#[test]
fn a_volfs_spelling_of_a_kept_path_is_kept() {
    let home = Home::new();
    let probe = LiveProbe::new();
    let guard = home.guard(&probe);
    let volfs = |dir: &str, rest: &str| {
        let meta = std::fs::metadata(dir).unwrap();
        format!("/.vol/{}/{}{rest}", meta.dev(), meta.ino())
    };
    for path in [
        volfs("/System", ""),
        volfs("/", "/System"),
        volfs("/Library/Keychains", "/System.keychain"),
        volfs(&home.at("Library/Keychains"), "/login.keychain-db"),
        volfs(&home.at("Library"), "/Keychains/login.keychain-db"),
    ] {
        assert_kept_everywhere(&guard, &probe, &path);
    }
}
