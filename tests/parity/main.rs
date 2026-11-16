#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("the Mole parity harness runs on macOS only");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
fn main() {
    if let Err(e) = cli::run(std::env::args().skip(1).collect()) {
        eprintln!("parity: {e}");
        std::process::exit(e.exit_code());
    }
}

#[cfg(target_os = "macos")]
mod cli {
    use dustpan_parity::candidates::{
        Parsed, SAVED_HOME, escaped_on_disk, missing_on_disk, parse_clean_list,
        parse_installer_debug, parse_purge_stdout,
    };
    use dustpan_parity::diff::{KnownDifferences, diff};
    use dustpan_parity::error::{Error, Result, io_err};
    use dustpan_parity::fixture_home::{Drift, Fixture, Manifest, drift, snapshot};
    use dustpan_parity::interrupt;
    use dustpan_parity::mole_runner::{
        DEFAULT_TIMEOUT, MoleCommand, MoleRun, MoleRunner, locate_mo, real_home,
    };
    use std::fs;
    use std::path::Path;
    use std::time::Duration;

    const STANDARD_MANIFEST: &str = include_str!("manifests/standard.toml");
    const KNOWN_DIFFERENCES: &str = include_str!("known-differences.toml");
    const DUSTPAN_NOTE: &str = "empty until M3: dustpan has no catalog to list candidates yet";
    const USAGE: &str = "usage: parity <clean|purge|installer|self-check> [--manifest FILE] [--whitelist HOME_RELATIVE_PATH]... [--timeout SECS] [--keep] [--save DIR]";

    struct Args {
        command: MoleCommand,
        manifest: Option<String>,
        whitelist: Vec<String>,
        timeout: Duration,
        keep: bool,
        save: Option<String>,
    }

    fn parse_args(raw: Vec<String>) -> Result<Args> {
        let usage = || Error::Usage(USAGE.into());
        let mut it = raw.into_iter();
        let command = it
            .next()
            .and_then(|c| MoleCommand::parse(&c))
            .ok_or_else(usage)?;
        let mut args = Args {
            command,
            manifest: None,
            whitelist: Vec::new(),
            timeout: DEFAULT_TIMEOUT,
            keep: false,
            save: None,
        };
        while let Some(flag) = it.next() {
            match flag.as_str() {
                "--manifest" => args.manifest = Some(it.next().ok_or_else(usage)?),
                "--whitelist" => args
                    .whitelist
                    .push(format!("home/{}", it.next().ok_or_else(usage)?)),
                "--timeout" => {
                    let secs = it.next().and_then(|s| s.parse().ok()).ok_or_else(usage)?;
                    args.timeout = Duration::from_secs(secs);
                }
                "--keep" => args.keep = true,
                "--save" => args.save = Some(it.next().ok_or_else(usage)?),
                _ => return Err(usage()),
            }
        }
        Ok(args)
    }

    fn parse_run(command: MoleCommand, mole: &MoleRun, home: &Path) -> Result<Parsed> {
        Ok(match command {
            MoleCommand::Clean => {
                parse_clean_list(mole.clean_list.as_deref().unwrap_or_default(), home)?
            }
            MoleCommand::Purge => parse_purge_stdout(&mole.stdout, home)?,
            MoleCommand::Installer => parse_installer_debug(&mole.stderr, home)?,
        })
    }

    fn check_interrupt() -> Result<()> {
        if interrupt::requested() {
            Err(Error::Interrupted)
        } else {
            Ok(())
        }
    }

    fn print_drift(drift: &Drift) {
        println!(
            "fixture untouched: {} ({} modified, {} removed, {} added)",
            if drift.is_clean() { "yes" } else { "NO" },
            drift.modified.len(),
            drift.removed.len(),
            drift.added.len()
        );
        println!(
            "host tools (brew, npm, python3) created {} files in the fixture home; not counted as drift",
            drift.host_tool_files
        );
        for (label, paths) in [
            ("modified", &drift.modified),
            ("removed", &drift.removed),
            ("added", &drift.added),
        ] {
            for p in paths {
                println!("  {label}: {}", p.display());
            }
        }
    }

    /// Builds a fixture, runs the sandbox self-test, and prints the home Mole
    /// would see and the real home it must not see. CI checks both lines.
    fn self_check() -> Result<()> {
        let mo = locate_mo(std::env::var("PARITY_MO").ok().as_deref())?;
        let fixture = Fixture::build(&Manifest::default())?;
        let runner = MoleRunner::new(mo, &fixture, DEFAULT_TIMEOUT)?;
        println!("home={}", fixture.home().display());
        println!("real-home={}", real_home()?.display());
        println!("version={}", runner.version()?);
        Ok(())
    }

    pub fn run(raw: Vec<String>) -> Result<()> {
        interrupt::install().map_err(io_err("install signal handlers"))?;
        if raw.first().map(String::as_str) == Some("self-check") {
            return self_check();
        }
        let args = parse_args(raw)?;
        let mo = locate_mo(std::env::var("PARITY_MO").ok().as_deref())?;
        let text = match &args.manifest {
            Some(p) => fs::read_to_string(p).map_err(io_err(format!("read manifest {p}")))?,
            None => STANDARD_MANIFEST.to_string(),
        };
        let mut manifest = Manifest::parse(&text)?;
        manifest.mole.whitelist.extend(args.whitelist);
        manifest.validate()?;

        let fixture = Fixture::build(&manifest)?;
        check_interrupt()?;
        let before = snapshot(fixture.root())?;
        let runner = MoleRunner::new(mo, &fixture, args.timeout)?;
        let version = runner.version()?;
        let mole = runner.run(args.command);
        let drift = drift(&before, &snapshot(fixture.root())?);
        check_interrupt()?;
        let mole = match mole {
            Ok(m) => m,
            Err(e) => {
                print_drift(&drift);
                return Err(e);
            }
        };
        let home = fixture.home();
        if let Some(dir) = &args.save {
            save_capture(dir, args.command, &mole, &home)?;
        }

        let parsed = parse_run(args.command, &mole, &home);
        let parsed = match parsed {
            Ok(p) => p,
            Err(e) => {
                print_drift(&drift);
                return Err(e);
            }
        };

        println!("mole: {version} (verified)");
        println!(
            "command: mo {} (dry run, {:.1}s)",
            args.command.name(),
            mole.wall.as_secs_f64()
        );
        println!(
            "paths are relative to the fixture home; {} rows counted under another row are not candidates",
            parsed.nested.len()
        );
        println!(
            "== host rows, outside the fixture and ignored ({}) ==",
            parsed.outside.len()
        );
        for p in &parsed.outside {
            println!("  {p}");
        }
        let mole_paths: Vec<&str> = parsed.inside.iter().map(|c| c.path.as_str()).collect();
        let known = KnownDifferences::parse(KNOWN_DIFFERENCES)?;
        let result = diff(mole_paths, std::iter::empty());
        print!(
            "{}",
            result.render(DUSTPAN_NOTE, &known, args.command.name())
        );
        print_drift(&drift);
        if args.keep {
            println!("fixture kept at {}", fixture.keep().display());
        }
        let resolved_outside = escaped_on_disk(&parsed, &home);
        let missing = if args.command == MoleCommand::Installer {
            missing_on_disk(&parsed, &home)
        } else {
            Vec::new()
        };
        let mut escaped = parsed.escaped;
        escaped.extend(
            resolved_outside
                .into_iter()
                .map(|p| format!("{p} (resolves outside the fixture home)")),
        );
        if !escaped.is_empty() {
            return Err(Error::Escaped(escaped));
        }
        if !missing.is_empty() {
            return Err(Error::MissingRows(missing));
        }
        if drift.is_clean() {
            Ok(())
        } else {
            Err(Error::Drift(format!(
                "{} modified, {} removed, {} added",
                drift.modified.len(),
                drift.removed.len(),
                drift.added.len()
            )))
        }
    }

    fn save_capture(dir: &str, cmd: MoleCommand, run: &MoleRun, home: &Path) -> Result<()> {
        let redact = |s: &str| s.replace(&*home.to_string_lossy(), SAVED_HOME);
        fs::create_dir_all(dir).map_err(io_err(format!("create {dir}")))?;
        let (name, body) = match (&run.clean_list, cmd) {
            (Some(list), _) => (format!("{}.clean-list.txt", cmd.name()), redact(list)),
            (None, MoleCommand::Installer) => {
                (format!("{}.stderr.txt", cmd.name()), redact(&run.stderr))
            }
            (None, _) => (format!("{}.stdout.txt", cmd.name()), redact(&run.stdout)),
        };
        let path = Path::new(dir).join(name);
        fs::write(&path, body).map_err(io_err(format!("write {}", path.display())))
    }
}
