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

#[test]
fn a_tilde_followed_by_doubled_separators_still_names_the_home_folder() {
    let wl = text("~//Projects/keep\n~///a\n~//\n");
    assert_eq!(
        patterns(&wl, Origin::User),
        [
            "/Users/alice/Projects/keep",
            "/Users/alice/a",
            "/Users/alice"
        ]
    );
}

#[test]
fn home_variables_expand_like_a_tilde() {
    let wl = text("$HOME/Projects/keep\n${HOME}/b\n$HOME\n${HOME}//c\n");
    assert_eq!(
        patterns(&wl, Origin::User),
        [
            "/Users/alice/Projects/keep",
            "/Users/alice/b",
            "/Users/alice",
            "/Users/alice/c"
        ]
    );
    let err = load(Read::Text("~/ok\n$HOMEX/y\n".into())).unwrap_err();
    assert!(err.to_string().contains("line 2"), "{err}");
}

fn judge_on(os: Os, home: &str, fs: &FakeProbe, wl: Whitelist, path: &str) -> Verdict {
    let env = |_: &str| None;
    let guard = Guard::new(
        GuardInputs {
            os,
            home: p(home),
            env: &env,
            whitelist: wl,
        },
        fs,
    )
    .unwrap();
    guard.pass(fs).evaluate(p(path)).verdict().clone()
}

fn kept(fs: &FakeProbe, entries: &str, cases: &[(&str, &str)]) {
    for &(path, want) in cases {
        let got = outcome(&judge(fs, text(entries), path));
        assert_eq!(got, want, "entries {entries:?}, path {path}");
    }
}

#[test]
fn question_marks_and_brackets_match_one_character_as_in_mole() {
    let fs = FakeProbe::new()
        .dir("/Users/alice/Library/Caches/com.foo.bar")
        .dir("/Users/alice/Library/Caches/com.foo.bbar")
        .dir("/Users/alice/Library/Caches/com.goo.bar")
        .dir("/Users/alice/Library/Caches/com.zoo.bar")
        .dir("/Users/alice/Library/Caches/v1")
        .dir("/Users/alice/Library/Caches/vx")
        .dir("/Users/alice/Library/Caches/Stra\u{df}e");
    let w = "whitelist/whitelisted";
    kept(
        &fs,
        "~/Library/Caches/com.foo.?ar\n",
        &[
            ("/Users/alice/Library/Caches/com.foo.bar", w),
            ("/Users/alice/Library/Caches/com.foo.bbar", "eligible"),
        ],
    );
    kept(
        &fs,
        "~/Library/Caches/com.[f-h]oo.bar\n",
        &[
            ("/Users/alice/Library/Caches/com.goo.bar", w),
            ("/Users/alice/Library/Caches/com.zoo.bar", "eligible"),
        ],
    );
    kept(
        &fs,
        "~/Library/Caches/v[!0-9]\n",
        &[
            ("/Users/alice/Library/Caches/vx", w),
            ("/Users/alice/Library/Caches/v1", "eligible"),
        ],
    );
    kept(
        &fs,
        "~/Library/Caches/com.[!z]oo.bar\n",
        &[
            ("/Users/alice/Library/Caches/com.foo.bar", w),
            (
                "/Users/alice/Library/Caches/com.zoo.bar",
                "whitelist/whitelisted",
            ),
        ],
    );
    kept(
        &fs,
        "~/Library/Caches/v[[:digit:]]\n",
        &[
            ("/Users/alice/Library/Caches/v1", w),
            ("/Users/alice/Library/Caches/vx", "eligible"),
        ],
    );
    kept(
        &fs,
        "~/Library/Caches/Stra?e\n",
        &[("/Users/alice/Library/Caches/Stra\u{df}e", w)],
    );
}

#[test]
fn a_bracket_in_a_real_folder_name_still_keeps_that_folder() {
    let fs = FakeProbe::new().dir("/Users/alice/Projects/mix/[2020] Album/disc");
    kept(
        &fs,
        "~/Projects/mix/[2020] Album\n",
        &[
            (
                "/Users/alice/Projects/mix/[2020] Album/disc",
                "whitelist/whitelisted",
            ),
            (
                "/Users/alice/Projects/mix",
                "whitelist/contains-whitelisted",
            ),
        ],
    );
}

#[test]
fn a_bracket_that_cannot_be_matched_as_mole_would_stops_the_load() {
    for bad in ["~/a/[[:nope:]]", "~/a/[z-a]", "~/a/[[=e=]]"] {
        let err = load(Read::Text(format!("~/ok\n{bad}\n"))).unwrap_err();
        let said = err.to_string();
        assert!(said.contains("line 2"), "{bad}: {said}");
    }
}

const NFC: &str = "Caf\u{e9}";
const NFD: &str = "Cafe\u{301}";

#[test]
fn an_entry_keeps_a_folder_stored_in_the_other_normalization_form() {
    let nfd_disk = FakeProbe::new().dir(&format!("/Users/alice/Docs/{NFD}/sub"));
    kept(
        &nfd_disk,
        &format!("~/Docs/{NFC}\n"),
        &[
            (
                &format!("/Users/alice/Docs/{NFD}/sub"),
                "whitelist/whitelisted",
            ),
            (&format!("/Users/alice/Docs/{NFD}"), "whitelist/whitelisted"),
        ],
    );
    kept(
        &nfd_disk,
        &format!("~/Docs/{NFC}/sub\n"),
        &[(
            &format!("/Users/alice/Docs/{NFD}"),
            "whitelist/contains-whitelisted",
        )],
    );
    let nfc_disk = FakeProbe::new().dir(&format!("/Users/alice/Docs/{NFC}/sub/f"));
    kept(
        &nfc_disk,
        &format!("~/Docs/{NFD}/sub\n"),
        &[
            (
                &format!("/Users/alice/Docs/{NFC}/sub/f"),
                "whitelist/whitelisted",
            ),
            (
                &format!("/Users/alice/Docs/{NFC}"),
                "whitelist/contains-whitelisted",
            ),
        ],
    );
}

#[test]
fn an_entry_keeps_a_folder_whose_name_differs_only_in_non_ascii_case() {
    let fs = FakeProbe::new().dir("/Users/alice/V/\u{e9}t\u{e9}/f");
    let w = "whitelist/whitelisted";
    kept(
        &fs,
        "~/V/\u{c9}T\u{c9}\n",
        &[("/Users/alice/V/\u{e9}t\u{e9}/f", w)],
    );
    kept(
        &fs,
        "~/V/E\u{301}*\n",
        &[("/Users/alice/V/\u{e9}t\u{e9}/f", w)],
    );
}

#[test]
fn an_entry_written_through_a_symlinked_folder_keeps_the_folder_behind_it() {
    let fs = FakeProbe::new()
        .symlink("/tmp", "private/tmp")
        .dir("/private/tmp/w/keep/x");
    kept(
        &fs,
        "/tmp/w/keep\n",
        &[
            ("/private/tmp/w/keep/x", "whitelist/whitelisted"),
            ("/private/tmp/w", "whitelist/contains-whitelisted"),
        ],
    );
    kept(
        &fs,
        "/tmp/w/keep/later/*\n",
        &[("/private/tmp/w/keep", "whitelist/contains-whitelisted")],
    );
}

#[test]
fn an_entry_that_is_a_link_keeps_what_lies_behind_it() {
    let fs = FakeProbe::new()
        .dir("/Users/alice/Library/Caches/build-1/x")
        .symlink("/Users/alice/latest", "/Users/alice/Library/Caches/build-1");
    kept(
        &fs,
        "~/latest\n",
        &[(
            "/Users/alice/Library/Caches/build-1/x",
            "whitelist/whitelisted",
        )],
    );
}

#[test]
fn a_folder_reached_through_a_link_is_matched_by_its_name_as_stored() {
    let fs = FakeProbe::new()
        .dir("/Users/alice/Library/Caches/[z-a]/*/x")
        .dir("/Users/alice/Library/Caches/[z-a]/other/x")
        .symlink("/Users/alice/latest", "/Users/alice/Library/Caches/[z-a]/*");
    kept(
        &fs,
        "~/latest\n",
        &[
            (
                "/Users/alice/Library/Caches/[z-a]/*/x",
                "whitelist/whitelisted",
            ),
            ("/Users/alice/Library/Caches/[z-a]/other/x", "eligible"),
        ],
    );
}

#[test]
fn an_entry_under_a_linked_home_root_keeps_the_physical_home_on_linux() {
    let fs = FakeProbe::new()
        .symlink("/home", "var/home")
        .dir("/var/home/alice/keep/x");
    let home = "/var/home/alice";
    let wl = whitelist::load(
        Os::Linux,
        &p(home),
        &p("/var/home/alice/.config/dustpan/whitelist"),
        Read::Text("/home/alice/keep\n".into()),
    )
    .unwrap();
    let v = judge_on(Os::Linux, home, &fs, wl, "/var/home/alice/keep/x");
    assert_eq!(outcome(&v), "whitelist/whitelisted");
}

#[test]
fn an_entry_spelled_through_the_data_volume_firmlink_keeps_the_root_spelling() {
    let fs = FakeProbe::new()
        .dir("/Users/alice/Projects/keep/child")
        .alias("/System/Volumes/Data/Users", "/Users");
    kept(
        &fs,
        "/System/Volumes/Data/Users/alice/Projects/keep\n",
        &[
            ("/Users/alice/Projects/keep/child", "whitelist/whitelisted"),
            ("/Users/alice/Projects", "whitelist/contains-whitelisted"),
        ],
    );
}

#[test]
fn an_entry_whose_folders_cannot_be_inspected_stops_the_guard_build() {
    let fs = FakeProbe::new()
        .dir("/Users/alice/Projects")
        .unreadable("/Users/alice/Locked");
    let env = |_: &str| None;
    let built = Guard::new(
        GuardInputs {
            os: Os::MacOs,
            home: p(HOME),
            env: &env,
            whitelist: text("~/Locked/keep\n"),
        },
        &fs,
    );
    let Err(err) = built else {
        panic!("a whitelist entry whose spellings cannot be found must not build a guard");
    };
    assert!(
        err.to_string().contains("/Users/alice/Locked/keep"),
        "{err}"
    );
}
