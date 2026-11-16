use dustpan_parity::candidates::{
    Parsed, SAVED_HOME, parse_clean_list, parse_installer_stdout, parse_purge_stdout,
};
use dustpan_parity::diff::{KnownDifferences, diff};
use dustpan_parity::error::SUPPORTED_MOLE;
use std::path::Path;

const CLEAN_163: &str = include_str!("data/mole-1.63.0/clean.clean-list.txt");
const PURGE_163: &str = include_str!("data/mole-1.63.0/purge.stdout.txt");
const INSTALLER_163: &str = include_str!("data/mole-1.63.0/installer.stdout.txt");

fn home() -> &'static Path {
    Path::new(SAVED_HOME)
}

fn paths(p: &Parsed) -> Vec<&str> {
    let mut v: Vec<&str> = p.inside.iter().map(|c| c.path.as_str()).collect();
    v.sort_unstable();
    v
}

#[test]
fn clean_list_keeps_fixture_rows_and_sets_host_rows_aside() {
    let parsed = parse_clean_list(CLEAN_163, home()).unwrap();
    let listed = paths(&parsed);
    for want in [
        "~/.Trash/old-report.pdf",
        "~/Library/Caches/Google",
        "~/Library/Caches/Firefox/Profiles",
        "~/Library/Caches/com.example.widget",
        "~/Library/Logs/ExampleApp",
        "~/Library/Logs/trap-logs",
    ] {
        assert!(listed.contains(&want), "{want} missing from {listed:?}");
    }
    assert!(!listed.iter().any(|p| p.contains("com.example.notes")));
    assert!(!listed.iter().any(|p| p.contains("Bookmarks")));
    assert!(!listed.iter().any(|p| p.contains("canary")));
    assert_eq!(parsed.outside.len(), 3, "{:?}", parsed.outside);
    assert!(
        parsed
            .outside
            .iter()
            .all(|p| p.starts_with('/') && !p.starts_with(SAVED_HOME))
    );
}

#[test]
fn npm_debug_log_names_are_masked_so_reruns_match() {
    let a = parse_clean_list(
        "/FIXTURE/home/.npm/_logs/2026-11-15T15_05_46_534Z-debug-0.log  # 2KB\n",
        home(),
    )
    .unwrap();
    let b = parse_clean_list(
        "/FIXTURE/home/.npm/_logs/2026-11-16T00_30_22_617Z-debug-0.log  # 2KB\n",
        home(),
    )
    .unwrap();
    assert_eq!(a, b);
    assert_eq!(a.inside[0].path, "~/.npm/_logs/<timestamp>-debug-0.log");
}

#[test]
fn clean_list_rows_counted_under_another_row_are_not_candidates() {
    let parsed = parse_clean_list(CLEAN_163, home()).unwrap();
    assert_eq!(parsed.nested, ["~/Library/Caches/Google/Chrome/Default"]);
    assert!(!paths(&parsed).contains(&"~/Library/Caches/Google/Chrome/Default"));
}

#[test]
fn purge_lists_the_old_artifacts_only() {
    let parsed = parse_purge_stdout(PURGE_163, home()).unwrap();
    assert_eq!(
        paths(&parsed),
        [
            "~/Projects/oldrust/target",
            "~/Projects/oldweb/node_modules"
        ]
    );
}

#[test]
fn installer_lists_dmg_pkg_and_installer_zip_only() {
    let parsed = parse_installer_stdout(INSTALLER_163).unwrap();
    assert_eq!(
        paths(&parsed),
        [
            "~/Downloads/Example-1.0.dmg",
            "~/Downloads/ExampleInstaller.zip",
            "~/Downloads/Setup.pkg"
        ]
    );
}

#[test]
fn parsers_fail_loudly_on_an_unknown_line_and_name_the_verified_versions() {
    let clean = parse_clean_list("=== A ===\nnot a row at all\n", home()).unwrap_err();
    let purge = parse_purge_stdout("✓ [DRY RUN] ~/x, 1MB\nSurprise banner\n", home()).unwrap_err();
    let installer = parse_installer_stdout("Totally new header\n").unwrap_err();
    for (err, line) in [(clean, 2), (purge, 2), (installer, 1)] {
        assert_eq!(err.line_no, line);
        let message = err.to_string();
        for version in SUPPORTED_MOLE {
            assert!(message.contains(version), "{message}");
        }
    }
}

#[test]
fn both_dry_run_banner_icons_are_accepted() {
    for icon in ["→", "○"] {
        let purge = format!(
            "{icon} DRY RUN MODE, No project artifacts will be removed\n\n✓ [DRY RUN] ~/p/node_modules, 1.1MB\n"
        );
        let installer = format!(
            "{icon} DRY RUN MODE, No installer files will be removed\n  ○ a.dmg 1.0MB | Downloads\n"
        );
        assert_eq!(parse_purge_stdout(&purge, home()).unwrap().inside.len(), 1);
        assert_eq!(parse_installer_stdout(&installer).unwrap().inside.len(), 1);
    }
}

#[test]
fn parsers_reject_a_row_with_a_bad_size() {
    assert!(parse_clean_list("/FIXTURE/home/x  # lots\n", home()).is_err());
    assert!(parse_purge_stdout("✓ [DRY RUN] ~/x, lots\n", home()).is_err());
    assert!(parse_installer_stdout("  ○ a.dmg lots | Downloads\n").is_err());
}

#[test]
fn installer_rejects_an_unknown_source_label() {
    assert!(parse_installer_stdout("  ○ a.dmg 1.0MB | Mystery\n").is_err());
}

#[test]
fn diff_splits_three_ways_and_dustpan_side_is_empty_today() {
    let d = diff(["~/a", "~/b"], ["~/b", "~/c"]);
    assert_eq!(d.only_mole, ["~/a"]);
    assert_eq!(d.only_dustpan, ["~/c"]);
    assert_eq!(d.both, ["~/b"]);

    let none = diff(["~/a"], std::iter::empty());
    let text = none.render("empty until M3", &KnownDifferences::default(), "clean");
    assert!(text.contains("only dustpan found (0)"));
    assert!(text.contains("empty until M3"));
}

#[test]
fn known_differences_start_with_com_example_notes() {
    let known = KnownDifferences::parse(include_str!("known-differences.toml")).unwrap();
    assert!(
        known
            .known
            .iter()
            .any(|k| k.command == "clean" && k.path.ends_with("com.example.notes"))
    );
}

#[cfg(unix)]
mod fixture {
    use dustpan_parity::error::Error;
    use dustpan_parity::fixture_home::{Fixture, Manifest, assert_under_temp, drift, snapshot};
    use std::fs;
    use std::time::{Duration, SystemTime};

    fn standard() -> Fixture {
        let manifest = Manifest::parse(include_str!("manifests/standard.toml")).unwrap();
        Fixture::build(&manifest).unwrap()
    }

    fn age(path: &std::path::Path) -> Duration {
        let modified = fs::symlink_metadata(path).unwrap().modified().unwrap();
        SystemTime::now().duration_since(modified).unwrap()
    }

    const DAY: u64 = 86_400;

    #[test]
    fn standard_fixture_has_old_and_fresh_projects_and_symlink_traps() {
        let f = standard();
        let h = f.home();
        assert!(
            age(&h.join("Projects/oldweb/node_modules/left-pad/index.js")).as_secs() > 29 * DAY
        );
        assert!(age(&h.join("Projects/oldweb/node_modules")).as_secs() > 29 * DAY);
        assert!(age(&h.join("Projects/freshweb/node_modules/right-pad/index.js")).as_secs() < DAY);
        for trap in [
            "Library/Caches/com.trap.victim",
            "Projects/trapweb/node_modules",
            "Downloads/trap.dmg",
        ] {
            let meta = fs::symlink_metadata(h.join(trap)).unwrap();
            assert!(meta.is_symlink(), "{trap}");
            assert!(
                fs::read_link(h.join(trap)).unwrap().starts_with(f.root()),
                "{trap}"
            );
        }
    }

    #[test]
    fn standard_fixture_has_a_real_installer_zip_and_a_plain_zip() {
        let f = standard();
        let installer = fs::read(f.home().join("Downloads/ExampleInstaller.zip")).unwrap();
        let plain = fs::read(f.home().join("Downloads/photos.zip")).unwrap();
        assert!(installer.windows(11).any(|w| w == b"Example.app"));
        assert!(!plain.windows(11).any(|w| w == b"Example.app"));
        assert_eq!(&installer[..4], b"PK\x03\x04");
    }

    #[test]
    fn fixture_lives_under_the_temp_folder_and_no_symlink_leaves_it() {
        let f = standard();
        assert_under_temp(f.root()).unwrap();
        for (path, kind) in snapshot(f.root()).unwrap() {
            if let dustpan_parity::fixture_home::Kind::Link { target, .. } = kind {
                assert!(
                    target.starts_with(f.root()),
                    "{} -> {}",
                    path.display(),
                    target.display()
                );
            }
        }
    }

    #[test]
    fn a_path_outside_the_temp_folder_is_refused() {
        let err = assert_under_temp(std::path::Path::new("/usr")).unwrap_err();
        assert!(matches!(err, Error::FixtureOutsideTemp(_)));
        assert_eq!(err.exit_code(), 6);
    }

    #[test]
    fn manifest_paths_must_stay_inside_the_fixture() {
        for bad in ["../x", "/etc/passwd", "a/../../x", ""] {
            let text = format!("[[file]]\npath = {bad:?}\nsize = 1\n");
            assert!(Manifest::parse(&text).is_err(), "{bad:?}");
        }
        assert!(Manifest::parse("[[file]]\npath = \"home/a\"\nsize = 1\nextra = 2\n").is_err());
    }

    #[test]
    fn same_manifest_gives_the_same_snapshot_shape() {
        let a = snapshot(standard().root()).unwrap();
        let b = snapshot(standard().root()).unwrap();
        assert_eq!(a.keys().collect::<Vec<_>>(), b.keys().collect::<Vec<_>>());
    }

    #[test]
    fn drift_reports_a_modified_removed_and_added_file() {
        let f = standard();
        let before = snapshot(f.root()).unwrap();
        fs::write(f.home().join("Documents/notes.txt"), "changed content").unwrap();
        fs::write(f.home().join("Documents/new.txt"), "x").unwrap();
        let mut after = snapshot(f.root()).unwrap();
        after.remove(std::path::Path::new("home/Downloads/Setup.pkg"));
        let d = drift(&before, &after);
        assert!(!d.is_clean());
        assert_eq!(
            d.modified,
            [std::path::PathBuf::from("home/Documents/notes.txt")]
        );
        assert_eq!(
            d.removed,
            [std::path::PathBuf::from("home/Downloads/Setup.pkg")]
        );
        assert_eq!(
            d.added,
            [std::path::PathBuf::from("home/Documents/new.txt")]
        );
    }

    #[test]
    fn mole_state_and_tmp_are_not_drift() {
        let f = standard();
        let before = snapshot(f.root()).unwrap();
        fs::create_dir_all(f.home().join(".config/mole")).unwrap();
        fs::write(f.home().join(".config/mole/clean-list.txt"), "x").unwrap();
        fs::write(f.tmp().join("scratch"), "x").unwrap();
        assert_eq!(
            drift(&before, &snapshot(f.root()).unwrap()),
            Default::default()
        );
    }

    #[test]
    fn whitelist_entries_become_absolute_paths_in_the_mole_config() {
        let manifest = Manifest::parse(
            "[[file]]\npath = \"home/Library/Caches/com.example.widget/Cache.bin\"\nsize = 1\n\
             [mole]\nwhitelist = [\"home/Library/Caches/com.example.widget\"]\n",
        )
        .unwrap();
        let f = Fixture::build(&manifest).unwrap();
        let text = fs::read_to_string(f.home().join(".config/mole/whitelist")).unwrap();
        assert_eq!(
            text.trim(),
            format!(
                "{}/home/Library/Caches/com.example.widget",
                f.root().display()
            )
        );
    }
}

#[cfg(target_os = "macos")]
mod runner {
    use dustpan_parity::error::Error;
    use dustpan_parity::fixture_home::{Fixture, Manifest};
    use dustpan_parity::mole_runner::{DryRunArgv, MoleCommand, MoleRunner, locate_mo};
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    const RUNNER_SRC: &str = include_str!("mole_runner.rs");
    const OTHER_SRC: [&str; 5] = [
        include_str!("candidates.rs"),
        include_str!("diff.rs"),
        include_str!("error.rs"),
        include_str!("fixture_home.rs"),
        include_str!("main.rs"),
    ];

    #[test]
    fn every_mole_command_line_carries_dry_run() {
        for cmd in MoleCommand::ALL {
            let argv = DryRunArgv::new(cmd);
            assert_eq!(argv.args()[0], cmd.name());
            assert!(
                argv.args().contains(&"--dry-run"),
                "{cmd:?}: {:?}",
                argv.args()
            );
        }
        assert_eq!(
            DryRunArgv::new(MoleCommand::Purge).args(),
            ["purge", "--dry-run", "--yes"]
        );
    }

    #[test]
    fn only_one_call_site_can_hand_arguments_to_mo() {
        let program_uses = RUNNER_SRC.matches(concat!("&self", ".mo")).count();
        assert_eq!(program_uses, 1, "mo must be spawned from exec_mo only");
        let dry = RUNNER_SRC.matches("Invocation::DryRun(").count();
        assert_eq!(
            dry, 2,
            "DryRun is matched in args() and built in run() only"
        );
        assert_eq!(
            RUNNER_SRC.matches(concat!("DryRunArgv", "::new(")).count(),
            1
        );
        for (i, src) in OTHER_SRC.iter().enumerate() {
            assert!(
                !src.contains(concat!("Command", "::new(")),
                "module {i} spawns a process"
            );
        }
    }

    fn fixture() -> Fixture {
        let manifest = Manifest::parse("[[file]]\npath = \"home/a.txt\"\nsize = 1\n").unwrap();
        Fixture::build(&manifest).unwrap()
    }

    fn fake_mo(dir: &Path, version: &str) -> PathBuf {
        let path = dir.join("mo");
        let script = format!(
            "#!/bin/sh\n\
             if [ \"$1\" = --version ]; then printf '\\nMole version {version}\\n'; exit 0; fi\n\
             {{ echo \"ARGS=$*\"; echo \"HOME=$HOME\"; env | sort | grep -v '^_=' | grep -v '^PATH=' | grep -v '^PWD=' | grep -v '^SHLVL=' | grep -v '^TMPDIR=' | cut -d= -f1 | tr '\\n' ' '; echo; cat; }} > \"$HOME/seen.txt\"\n\
             mkdir -p \"$HOME/.config/mole\"; echo '# list' > \"$HOME/.config/mole/clean-list.txt\"\n"
        );
        fs::write(&path, script).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[test]
    fn the_runner_passes_dry_run_a_fixture_home_a_scrubbed_env_and_empty_stdin() {
        let scratch = tempfile::tempdir().unwrap();
        let mo = fake_mo(scratch.path(), "1.63.0");
        let f = fixture();
        let runner = MoleRunner::new(mo, &f, Duration::from_secs(30)).unwrap();
        assert_eq!(runner.version().unwrap(), "1.63.0");
        for cmd in MoleCommand::ALL {
            runner.run(cmd).unwrap();
            let seen = fs::read_to_string(f.home().join("seen.txt")).unwrap();
            let mut lines = seen.lines();
            let args = lines.next().unwrap();
            assert!(
                args.starts_with(&format!("ARGS={} --dry-run", cmd.name())),
                "{args}"
            );
            assert_eq!(
                lines.next().unwrap(),
                format!("HOME={}", f.home().display())
            );
            assert_eq!(
                lines.next().unwrap().trim(),
                "HOME LANG TERM",
                "environment was not scrubbed"
            );
            assert_eq!(lines.next(), None, "stdin was not empty");
        }
    }

    #[test]
    fn an_unverified_mole_version_is_refused_with_exit_4() {
        let scratch = tempfile::tempdir().unwrap();
        let mo = fake_mo(scratch.path(), "9.9.9");
        let f = fixture();
        let runner = MoleRunner::new(mo, &f, Duration::from_secs(30)).unwrap();
        let err = runner.version().unwrap_err();
        assert!(matches!(err, Error::MoleVersion { .. }));
        assert_eq!(err.exit_code(), 4);
    }

    #[test]
    fn a_missing_mo_exits_3_with_an_install_hint() {
        let err = locate_mo(Some("/nonexistent/mo")).unwrap_err();
        assert_eq!(err.exit_code(), 3);
        assert!(err.to_string().contains("brew install mole"));
    }

    #[test]
    fn a_hung_mole_is_killed_at_the_time_bound() {
        let scratch = tempfile::tempdir().unwrap();
        let mo = scratch.path().join("mo");
        fs::write(&mo, "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'Mole version 1.63.0'; exit 0; fi\nsleep 60\n").unwrap();
        fs::set_permissions(&mo, fs::Permissions::from_mode(0o755)).unwrap();
        let f = fixture();
        let runner = MoleRunner::new(mo, &f, Duration::from_millis(500)).unwrap();
        let started = std::time::Instant::now();
        let err = runner.run(MoleCommand::Installer).err().unwrap();
        assert!(err.to_string().contains("timed out"), "{err}");
        assert!(started.elapsed() < Duration::from_secs(10));
    }
}
