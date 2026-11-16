//! The Windows table against an in-memory file system: `safety-invariants.md` section 2.9.
//! Paths are host syntax, so these cases run on the Windows CI runners only.
#![cfg(windows)]

use dustpan_core::guard::{Guard, GuardInputs, Homes, KeptReason, PolicyError, Verdict};
use dustpan_core::path::AbsPath;
use dustpan_core::paths::Os;
use dustpan_core::probe::fake::FakeProbe;
use dustpan_core::whitelist::Whitelist;

const HOME: &str = r"C:\Users\alice";

fn p(s: &str) -> AbsPath {
    AbsPath::parse(s.as_ref()).unwrap()
}

fn windows() -> FakeProbe {
    FakeProbe::new()
        .dir(r"C:\Windows\System32")
        .dir(r"C:\Program Files\App")
        .dir(r"C:\ProgramData\Microsoft\Windows Defender")
        .dir(r"C:\Users\alice\AppData\Local\Microsoft\Credentials")
        .dir(r"C:\Users\alice\AppData\Local\Temp")
        .file(r"C:\Users\alice\Documents\report.txt")
        .dir(r"D:\work\project")
        // As Windows ships it. The profiles behind the junction stay judged by their own rows.
        .symlink(r"C:\Documents and Settings", r"C:\Users")
}

fn env(key: &str) -> Option<String> {
    let value = match key {
        "SystemRoot" | "windir" => r"C:\Windows",
        "SystemDrive" => "C:",
        "ProgramFiles" => r"C:\Program Files",
        "ProgramFiles(x86)" => r"C:\Program Files (x86)",
        "ProgramData" => r"C:\ProgramData",
        "LOCALAPPDATA" => r"C:\Users\alice\AppData\Local",
        _ => return None,
    };
    Some(value.to_owned())
}

fn guard(fs: &FakeProbe, env: &dyn Fn(&str) -> Option<String>) -> Guard {
    Guard::new(
        GuardInputs {
            os: Os::Windows,
            homes: Homes::new(p(HOME), None),
            env,
            whitelist: Whitelist::defaults(Os::Windows, &p(HOME)),
        },
        fs,
    )
    .unwrap()
}

fn outcome(verdict: &Verdict) -> String {
    match verdict.kept() {
        Some(reason) => format!("{}/{}", reason.layer().code(), reason.code()),
        None => "eligible".to_owned(),
    }
}

fn assert_outcomes(guard: &Guard, fs: &FakeProbe, cases: &[(&str, &str)]) {
    let mut pass = guard.pass(fs);
    let wrong: Vec<String> = cases
        .iter()
        .filter_map(|(path, want)| {
            let got = outcome(pass.evaluate(p(path)).verdict());
            (got != *want).then(|| format!("{path}: got {got}, want {want}"))
        })
        .collect();
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

const CASES: &[(&str, &str)] = &[
    (r"C:\Windows", "critical/critical-system-path"),
    (r"C:\Windows\System32", "critical/critical-system-path"),
    (
        r"C:\Windows\SysWOW64\x.dll",
        "critical/critical-system-path",
    ),
    (
        r"c:\windows\system32\drivers",
        "critical/critical-system-path",
    ),
    (r"C:\Program Files", "critical/critical-system-path"),
    (r"C:\Program Files\App\x", "critical/critical-system-path"),
    (
        r"C:\Program Files (x86)\App",
        "critical/critical-system-path",
    ),
    (
        r"C:\Program Files\Windows Defender\x",
        "critical/critical-system-path",
    ),
    (
        r"C:\ProgramData\Microsoft\Windows Defender\Scans",
        "critical/critical-system-path",
    ),
    (r"C:\", "critical/system-folder"),
    (r"C:\Users", "critical/system-folder"),
    (r"C:\Users\alice", "critical/home-root"),
    (r"C:\USERS\ALICE", "critical/home-root"),
    (r"C:\Users\bob", "critical/home-root"),
    (r"C:\Users\alice\Documents", "critical/home-folder"),
    (r"C:\Users\alice\Desktop", "critical/home-folder"),
    (r"C:\Users\alice\AppData", "critical/home-folder"),
    (r"C:\Users\alice\AppData\Local", "critical/home-folder"),
    (r"C:\Users\alice\Documents\report.txt", "eligible"),
    (r"C:\Users\alice\AppData\Local\Temp\x.tmp", "eligible"),
    (
        r"C:\Users\alice\AppData\Local\Microsoft\Credentials",
        "protection/user-data",
    ),
    (
        r"C:\Users\alice\AppData\Roaming\Microsoft\Protect\S-1-5\key",
        "protection/user-data",
    ),
    (
        r"C:\Users\alice\AppData\Roaming\microsoft\crypto\RSA",
        "protection/user-data",
    ),
    (
        r"C:\Users\alice\AppData\Local\Microsoft\Vault\x",
        "protection/user-data",
    ),
    (
        r"C:\Users\alice\AppData\Roaming\Microsoft\SystemCertificates\My",
        "protection/user-data",
    ),
    (
        r"C:\Users\alice\AppData\Roaming\Microsoft\Windows\Start Menu\Programs",
        "protection/user-data",
    ),
    // Whole segments only: `Cryptography` is not `Crypto`.
    (
        r"C:\Users\alice\AppData\Local\Microsoft\Cryptography\x",
        "eligible",
    ),
    (r"D:\work\project\target", "eligible"),
];

#[test]
fn protected_prefixes_the_profile_root_and_protected_segments_are_kept() {
    let fs = windows();
    assert_outcomes(&guard(&fs, &env), &fs, CASES);
}

#[test]
fn literal_rows_hold_when_the_environment_is_empty() {
    let fs = windows();
    let empty = |_: &str| None;
    assert_outcomes(&guard(&fs, &empty), &fs, CASES);
}

#[test]
fn a_redirected_system_root_adds_protection_and_removes_none() {
    let fs = windows().dir(r"E:\WinNT\System32");
    let redirected = |key: &str| match key {
        "SystemRoot" | "windir" => Some(r"E:\WinNT".to_owned()),
        "SystemDrive" => Some("E:".to_owned()),
        _ => None,
    };
    let guard = guard(&fs, &redirected);
    assert_outcomes(
        &guard,
        &fs,
        &[
            (r"E:\WinNT\System32", "critical/critical-system-path"),
            (r"E:\Users\bob", "critical/home-root"),
            (r"C:\Windows\System32", "critical/critical-system-path"),
        ],
    );
}

#[test]
fn a_linked_ancestor_is_kept() {
    let fs = windows().symlink(r"C:\Users\alice\junction", r"C:\Windows");
    let verdict = guard(&fs, &env)
        .pass(&fs)
        .evaluate(p(r"C:\Users\alice\junction\System32"))
        .verdict()
        .clone();
    assert_eq!(outcome(&verdict), "shape/symlink-ancestor");
    assert_eq!(
        verdict.to_string(),
        r"Kept · symlinked ancestor C:\Users\alice\junction"
    );
}

#[test]
fn a_profile_that_is_a_system_tree_means_no_guard() {
    for home in [r"C:\", r"C:\Windows", r"C:\Users"] {
        let built = Guard::new(
            GuardInputs {
                os: Os::Windows,
                homes: Homes::new(p(home), None),
                env: &env,
                whitelist: Whitelist::defaults(Os::Windows, &p(home)),
            },
            &windows(),
        );
        assert!(
            matches!(&built, Err(PolicyError::HomeIsCritical(h)) if *h == p(home)),
            "{home}"
        );
    }
}

#[test]
fn docker_state_keeps_the_folders_that_hold_it() {
    let fs = windows().dir(r"C:\Users\alice\AppData\Local\Docker\wsl");
    let guard = guard(&fs, &env);
    let verdict = guard
        .pass(&fs)
        .evaluate(p(r"C:\Users\alice\AppData\Local\Docker\wsl\data.vhdx"))
        .verdict()
        .clone();
    assert!(
        matches!(&verdict, Verdict::Kept(KeptReason::Protected(_))),
        "{verdict:?}"
    );
}

fn load(body: &str) -> Result<Whitelist, dustpan_core::whitelist::LoadError> {
    dustpan_core::whitelist::load(
        Os::Windows,
        &p(HOME),
        &p(r"C:\Users\alice\AppData\Roaming\dustpan\whitelist"),
        dustpan_core::config::Read::Text(body.into()),
    )
}

#[test]
fn a_tilde_with_doubled_backslashes_still_names_the_home_folder() {
    let wl = load("~\\\\Projects\\keep\r\n~/\\x\r\n").unwrap();
    let named: Vec<String> = wl
        .entries()
        .iter()
        .take(2)
        .map(|e| match &e.pattern {
            dustpan_core::whitelist::Pattern::Path(path) => path.to_string(),
            dustpan_core::whitelist::Pattern::FinderMetadata => String::new(),
        })
        .collect();
    assert_eq!(
        named,
        [r"C:\Users\alice\Projects\keep", r"C:\Users\alice\x"]
    );
}

#[test]
fn a_tilde_followed_by_a_drive_stops_the_load() {
    let err = load("~/ok\n~/D:\\work\n").unwrap_err();
    assert!(err.to_string().contains("line 2"), "{err}");
}
