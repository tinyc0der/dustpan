#![cfg(unix)]

use dustpan_core::config::Read;
use dustpan_core::guard::{Guard, GuardInputs, KeptReason, Layer, Source, Verdict};
use dustpan_core::path::AbsPath;
use dustpan_core::paths::Os;
use dustpan_core::probe::fake::FakeProbe;
use dustpan_core::whitelist::{self, LoadError, Origin, Pattern, Whitelist};

const HOME: &str = "/Users/alice";
const FILE: &str = "/Users/alice/.config/dustpan/whitelist";

fn p(s: &str) -> AbsPath {
    AbsPath::parse(s.as_ref()).unwrap()
}

fn load(read: Read) -> Result<Whitelist, LoadError> {
    whitelist::load(Os::MacOs, &p(HOME), &p(FILE), read)
}

fn text(body: &str) -> Whitelist {
    load(Read::Text(body.to_owned())).unwrap()
}

fn patterns(wl: &Whitelist, origin: Origin) -> Vec<String> {
    wl.entries()
        .iter()
        .filter(|e| e.origin == origin)
        .map(|e| match &e.pattern {
            Pattern::Path(path) => path.to_string(),
            Pattern::FinderMetadata => "FINDER_METADATA".to_owned(),
        })
        .collect()
}

#[test]
fn tilde_expands_to_the_home_folder() {
    let wl = text("~/Projects/keep\n~\n/opt/data\n");
    assert_eq!(
        patterns(&wl, Origin::User),
        ["/Users/alice/Projects/keep", "/Users/alice", "/opt/data"]
    );
}

#[test]
fn comments_and_blank_lines_are_skipped() {
    let wl = text("# my keeps\n\n   \n  # indented comment\n~/a\n\n#~/b\n");
    assert_eq!(patterns(&wl, Origin::User), ["/Users/alice/a"]);
}

#[test]
fn crlf_endings_and_trailing_spaces_do_not_change_an_entry() {
    let wl = text("~/a  \r\n~/b\t \r\n~/c\r\n");
    assert_eq!(
        patterns(&wl, Origin::User),
        ["/Users/alice/a", "/Users/alice/b", "/Users/alice/c"]
    );
}

#[test]
fn a_byte_order_mark_does_not_hide_the_first_entry() {
    let wl = text("\u{feff}~/a\n");
    assert_eq!(patterns(&wl, Origin::User), ["/Users/alice/a"]);
}

#[test]
fn a_missing_file_leaves_the_built_in_defaults_and_no_source() {
    let wl = load(Read::Missing).unwrap();
    assert_eq!(wl.file(), None);
    assert!(patterns(&wl, Origin::User).is_empty());
    let defaults = patterns(&wl, Origin::Default);
    assert_eq!(defaults.len(), 16, "{defaults:?}");
    assert!(defaults.contains(&"/Users/alice/.gradle/caches/*".to_owned()));
    assert!(defaults.contains(&"FINDER_METADATA".to_owned()));
    assert!(!patterns(&wl, Origin::Safety).is_empty());
}

#[test]
fn a_user_file_replaces_the_defaults_and_keeps_the_hard_safety_entries() {
    let wl = text("~/only\n");
    assert_eq!(wl.file(), Some(&p(FILE)));
    assert!(patterns(&wl, Origin::Default).is_empty());
    let safety = patterns(&wl, Origin::Safety);
    assert!(safety.contains(&"/Users/alice/Library/Caches/kitty".to_owned()));
    assert!(safety.contains(&"/Users/alice/Library/Caches/CloudKit*".to_owned()));
    assert!(safety.contains(&"FINDER_METADATA".to_owned()));
}

#[test]
fn an_unreadable_file_is_an_error_that_names_the_file_not_an_empty_list() {
    let err = load(Read::Unreadable {
        why: "Permission denied (os error 13)".into(),
    })
    .unwrap_err();
    let said = err.to_string();
    assert!(said.contains(FILE), "{said}");
    assert!(said.contains("Permission denied"), "{said}");
}

#[test]
fn an_entry_that_cannot_be_applied_stops_the_load_and_names_its_line() {
    for bad in ["relative/path", "~/a/../b", "~bob/x"] {
        let err = load(Read::Text(format!("~/ok\n{bad}\n"))).unwrap_err();
        let said = err.to_string();
        assert!(said.contains(FILE), "{bad}: {said}");
        assert!(said.contains("line 2"), "{bad}: {said}");
    }
}

fn mac() -> FakeProbe {
    FakeProbe::new()
        .dir("/Users/alice/Library/Caches/com.foo.bar")
        .dir("/Users/alice/Library/Caches/com.foobar")
        .dir("/Users/alice/Library/Caches/kitty/a")
        .dir("/Users/alice/Library/Caches/Other")
        .dir("/Users/alice/Projects/keep/child")
        .dir("/Users/alice/Projects/keeper")
        .dir("/Users/alice/.gradle/caches/x")
        .dir("/Users/alice/.gradle/daemon")
}

fn judge(fs: &FakeProbe, wl: Whitelist, path: &str) -> Verdict {
    let env = |_: &str| None;
    let guard = Guard::new(
        GuardInputs {
            os: Os::MacOs,
            home: p(HOME),
            env: &env,
            whitelist: wl,
        },
        fs,
    )
    .unwrap();
    guard.pass(fs).evaluate(p(path)).verdict().clone()
}

fn outcome(v: &Verdict) -> String {
    match v.kept() {
        Some(r) => format!("{}/{}", r.layer().code(), r.code()),
        None => "eligible".to_owned(),
    }
}

#[test]
fn a_default_entry_keeps_what_it_names_with_the_default_source() {
    let v = judge(
        &mac(),
        load(Read::Missing).unwrap(),
        "/Users/alice/.gradle/caches/x",
    );
    assert_eq!(v.to_string(), "Kept · protected by default whitelist");
    let reason = v.kept().unwrap();
    assert_eq!(reason.layer(), Layer::Whitelist);
    assert_eq!(reason.source(), Some(Source::BuiltIn));
}

#[test]
fn a_user_entry_keeps_its_children_and_names_the_file() {
    let fs = mac();
    let wl = text("~/Projects/keep\n");
    let v = judge(&fs, wl.clone(), "/Users/alice/Projects/keep/child");
    assert_eq!(outcome(&v), "whitelist/whitelisted");
    let said = v.to_string();
    assert!(
        said.contains(FILE) && said.contains("/Users/alice/Projects/keep"),
        "{said}"
    );
    let file = p(FILE);
    assert_eq!(v.kept().unwrap().source(), Some(Source::File(&file)));
    assert_eq!(
        outcome(&judge(&fs, wl, "/Users/alice/Projects/keeper")),
        "eligible",
        "a sibling that shares a prefix is not inside the entry"
    );
}

#[test]
fn a_folder_that_holds_a_whitelisted_path_is_kept_too() {
    let fs = mac();
    let wl = text("~/Projects/keep/child\n");
    let v = judge(&fs, wl, "/Users/alice/Projects/keep");
    assert_eq!(outcome(&v), "whitelist/contains-whitelisted");
    assert!(v.to_string().contains("/Users/alice/Projects/keep/child"));
}

#[test]
fn a_glob_keeps_what_it_matches_and_not_a_name_that_only_shares_a_prefix() {
    let fs = mac();
    let wl = text("~/Library/Caches/com.foo.*\n");
    assert_eq!(
        outcome(&judge(
            &fs,
            wl.clone(),
            "/Users/alice/Library/Caches/com.foo.bar"
        )),
        "whitelist/whitelisted"
    );
    assert_eq!(
        outcome(&judge(&fs, wl, "/Users/alice/Library/Caches/com.foobar")),
        "eligible"
    );
}

#[test]
fn a_user_file_that_omits_a_hard_safety_entry_still_keeps_it() {
    let fs = mac();
    let wl = text("~/Projects/keep\n");
    let v = judge(&fs, wl, "/Users/alice/Library/Caches/kitty/a");
    assert_eq!(outcome(&v), "whitelist/whitelisted");
    assert!(matches!(v.kept(), Some(KeptReason::Whitelisted(_))));
    assert_eq!(v.kept().unwrap().source(), Some(Source::BuiltIn));
}

#[test]
fn defaults_stop_applying_once_a_user_file_exists() {
    let fs = mac();
    let wl = text("~/Projects/keep\n");
    assert_eq!(
        outcome(&judge(&fs, wl, "/Users/alice/.gradle/caches/x")),
        "eligible"
    );
}

#[test]
fn one_pass_judges_paths_that_share_ancestors_like_separate_passes() {
    let fs = FakeProbe::new()
        .dir("/Users/alice/Projects/keep/deep/x/y")
        .dir("/Users/alice/Projects/keep/other")
        .dir("/Users/alice/Projects/keeper")
        .dir("/Users/alice/Projects/keep/deep/z");
    let env = |_: &str| None;
    let guard = Guard::new(
        GuardInputs {
            os: Os::MacOs,
            home: p(HOME),
            env: &env,
            whitelist: text("~/Projects/keep/deep/*\n"),
        },
        &fs,
    )
    .unwrap();
    let mut pass = guard.pass(&fs);
    for (path, want) in [
        (
            "/Users/alice/Projects/keep/deep/x/y",
            "whitelist/whitelisted",
        ),
        ("/Users/alice/Projects/keep/deep/z", "whitelist/whitelisted"),
        (
            "/Users/alice/Projects/keep/deep",
            "whitelist/contains-whitelisted",
        ),
        ("/Users/alice/Projects/keep/other", "eligible"),
        ("/Users/alice/Projects/keeper", "eligible"),
    ] {
        assert_eq!(outcome(pass.evaluate(p(path)).verdict()), want, "{path}");
    }
}

#[test]
fn the_finder_sentinel_keeps_ds_store_files() {
    let fs = FakeProbe::new().file("/Users/alice/Documents/.DS_Store");
    let v = judge(
        &fs,
        load(Read::Missing).unwrap(),
        "/Users/alice/Documents/.DS_Store",
    );
    assert_eq!(outcome(&v), "whitelist/whitelisted");
}
