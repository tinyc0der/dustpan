use crate::error::{Error, Result, SUPPORTED_MOLE, io_err};
use crate::fixture_home::{Fixture, assert_under_temp};
use crate::interrupt;
use std::fs::{self, File};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};
use tempfile::TempDir;

const SANDBOX_PROFILE: &str = include_str!("deny-writes.sb");
/// Creates `$1` exclusively (noclobber is O_EXCL), so a file that already
/// exists is never touched and never removed. Exit 5: the create was refused.
/// Exit 3: it succeeded and the probe was removed. Exit 4: it succeeded and
/// the removal failed, leaving the probe behind.
pub const CREATE_PROBE: &str =
    "set -C; /usr/bin/true > \"$1\" || exit 5; /bin/rm -f -- \"$1\" || exit 4; exit 3";
const PROBE_DIRS: &[&str] = &[
    "",
    "Library",
    "Library/Caches",
    "Library/Logs",
    "Library/Application Support",
    ".config",
    ".cache",
];
const DENIED: &str = "Operation not permitted";
const SANDBOX_EXEC: &str = "/usr/bin/sandbox-exec";
const SYSTEM_PATH: &str = "/usr/bin:/bin:/usr/sbin:/sbin";
// /bin/ps is setuid, and the sandbox refuses to exec setuid programs. Without a
// process table Mole's live reverse-DNS cache guard cannot rule out a running
// owner and skips every `Library/Caches/<bundle id>` folder. The fixture runs
// no apps, so an empty table is the true answer.
const PS_SHIM: &str = "#!/bin/sh\nprintf '  PID  PPID COMM ARGS\\n'\n";
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoleCommand {
    Clean,
    Purge,
    Installer,
}

impl MoleCommand {
    pub const ALL: [MoleCommand; 3] = [
        MoleCommand::Clean,
        MoleCommand::Purge,
        MoleCommand::Installer,
    ];

    pub fn name(self) -> &'static str {
        match self {
            MoleCommand::Clean => "clean",
            MoleCommand::Purge => "purge",
            MoleCommand::Installer => "installer",
        }
    }

    pub fn parse(s: &str) -> Option<MoleCommand> {
        Self::ALL.into_iter().find(|c| c.name() == s)
    }

    fn extra_flags(self) -> &'static [&'static str] {
        match self {
            // Installer paths exist only in the `--debug` trace on stderr.
            MoleCommand::Installer => &["--debug"],
            MoleCommand::Clean | MoleCommand::Purge => &[],
        }
    }
}

/// The only way to name a Mole command line. `DryRunArgv::new` is the one place
/// that builds arguments, and it always puts `--dry-run` in them.
#[derive(Debug, PartialEq, Eq)]
pub struct DryRunArgv(Vec<&'static str>);

impl DryRunArgv {
    pub fn new(cmd: MoleCommand) -> DryRunArgv {
        let mut argv = vec![cmd.name(), "--dry-run"];
        argv.extend_from_slice(cmd.extra_flags());
        DryRunArgv(argv)
    }

    pub fn args(&self) -> &[&'static str] {
        &self.0
    }
}

enum Invocation<'a> {
    Version,
    DryRun(&'a DryRunArgv),
}

impl Invocation<'_> {
    fn args(&self) -> &[&'static str] {
        match self {
            Invocation::Version => &["--version"],
            Invocation::DryRun(argv) => argv.args(),
        }
    }
}

pub fn locate_mo(env_override: Option<&str>) -> Result<PathBuf> {
    if let Some(p) = env_override {
        let p = PathBuf::from(p);
        return if p.is_file() {
            Ok(p)
        } else {
            Err(Error::MoleMissing(format!(
                "PARITY_MO={} is not a file",
                p.display()
            )))
        };
    }
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path)
        .map(|d| d.join("mo"))
        .find(|p| p.is_file())
        .ok_or_else(|| Error::MoleMissing("`mo` was not found on PATH".into()))
}

pub struct MoleRun {
    pub stdout: String,
    pub stderr: String,
    pub wall: Duration,
    pub clean_list: Option<String>,
}

pub struct MoleRunner {
    mo: PathBuf,
    home: PathBuf,
    scratch: TempDir,
    profile: PathBuf,
    timeout: Duration,
}

/// The home folder the user database names for this user, never `$HOME`.
pub fn real_home() -> Result<PathBuf> {
    let out = Command::new("/bin/sh")
        .args(["-c", "eval echo \"~$(/usr/bin/id -un)\""])
        .env_clear()
        .output()
        .map_err(io_err("look up the real home"))?;
    let home = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if !out.status.success() || !home.starts_with('/') || home.len() < 2 {
        return Err(Error::Sandbox(format!(
            "could not read the real home from the user database: {home:?}"
        )));
    }
    Ok(PathBuf::from(home))
}

/// The write-deny profile plus a read deny on the real home. Tools such as
/// `simctl` and `node` find the home from the user database and ignore `$HOME`.
pub fn sandbox_profile(real_home: &Path, mo: &Path) -> Result<String> {
    let mut roots = vec![real_home.to_path_buf()];
    if let Ok(canonical) = real_home.canonicalize()
        && canonical != real_home
    {
        roots.push(canonical);
    }
    let mut profile = SANDBOX_PROFILE.to_string();
    for root in &roots {
        profile.push_str(&format!(
            "(deny file-read-data (subpath {}))\n",
            quote(root)?
        ));
    }
    let mo_root = mo.parent().unwrap_or(mo);
    let mo_root = mo_root.canonicalize().unwrap_or(mo_root.to_path_buf());
    if roots.iter().any(|r| mo_root.starts_with(r)) {
        profile.push_str(&format!(
            "(allow file-read-data (subpath {}))\n",
            quote(&mo_root)?
        ));
    }
    Ok(profile)
}

fn random_hex() -> Result<String> {
    let mut bytes = [0u8; 8];
    File::open("/dev/urandom")
        .and_then(|mut f| std::io::Read::read_exact(&mut f, &mut bytes))
        .map_err(io_err("read /dev/urandom"))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

fn quote(p: &Path) -> Result<String> {
    let text = p.to_string_lossy();
    if text.contains(['"', '\\']) || text.chars().any(char::is_control) {
        return Err(Error::Sandbox(format!(
            "cannot write {text:?} into a sandbox profile"
        )));
    }
    Ok(format!("\"{text}\""))
}

impl MoleRunner {
    pub fn new(mo: PathBuf, fixture: &Fixture, timeout: Duration) -> Result<MoleRunner> {
        let real = real_home()?;
        let profile = sandbox_profile(&real, &mo)?;
        Self::with_profile(mo, fixture, timeout, &real, &profile)
    }

    /// Builds a runner around any profile and proves it blocks writes to and
    /// reads of `real_home`. A profile that allows either is refused.
    pub fn with_profile(
        mo: PathBuf,
        fixture: &Fixture,
        timeout: Duration,
        real_home: &Path,
        profile_text: &str,
    ) -> Result<MoleRunner> {
        assert_under_temp(fixture.root())?;
        let scratch = tempfile::Builder::new()
            .prefix("dustpan-parity-run.")
            .tempdir()
            .map_err(io_err("create scratch folder"))?;
        let profile = scratch.path().join("deny-writes.sb");
        fs::write(&profile, profile_text).map_err(io_err("write sandbox profile"))?;
        let shim = scratch.path().join("shims");
        fs::create_dir(&shim).map_err(io_err("create shim folder"))?;
        let ps = shim.join("ps");
        fs::write(&ps, PS_SHIM).map_err(io_err("write ps shim"))?;
        fs::set_permissions(&ps, fs::Permissions::from_mode(0o755))
            .map_err(io_err("chmod ps shim"))?;
        let runner = MoleRunner {
            mo,
            home: fixture.home(),
            scratch,
            profile,
            timeout,
        };
        runner.prove_sandbox(real_home)?;
        Ok(runner)
    }

    fn prove_sandbox(&self, real_home: &Path) -> Result<()> {
        for sub in PROBE_DIRS {
            let dir = real_home.join(sub);
            if dir.is_dir() {
                self.probe_create(&dir)?;
            }
        }
        self.probe_existing_entry(real_home)?;
        let home_text = real_home.to_string_lossy().into_owned();
        let read = self.sandboxed(Path::new("/bin/ls"), &[&home_text], "probe-read")?;
        if read.exit == Some(0) || !read.stderr.contains(DENIED) {
            return Err(Error::Sandbox(format!(
                "reading {home_text} was not denied inside the sandbox (exit {:?}, stderr {:?})",
                read.exit,
                read.stderr.trim()
            )));
        }
        let tmp_probe = self.tmp().join(".probe");
        let inside = self.sandboxed(
            Path::new("/usr/bin/touch"),
            &[&tmp_probe.to_string_lossy()],
            "probe-fixture",
        )?;
        if inside.exit != Some(0) {
            return Err(Error::Sandbox("a write inside the fixture failed".into()));
        }
        let seen = self.sandboxed(Path::new("/usr/bin/printenv"), &["HOME"], "probe-home")?;
        let seen_home = PathBuf::from(seen.stdout.trim());
        if seen_home != self.home {
            return Err(Error::Sandbox(format!(
                "the sandboxed HOME is {} and not the fixture home",
                seen_home.display()
            )));
        }
        assert_under_temp(&seen_home)?;
        Ok(())
    }

    /// A create in `dir` must be refused. The sandboxed shell removes the probe
    /// itself if the create is allowed, so a weak profile leaves at most the
    /// loudly reported path behind.
    fn probe_create(&self, dir: &Path) -> Result<()> {
        let probe = dir.join(format!(".dustpan-parity-probe-{}", random_hex()?));
        let probe_text = probe.to_string_lossy().into_owned();
        let out = self.sandboxed(
            Path::new("/bin/sh"),
            &["-c", CREATE_PROBE, "probe", &probe_text],
            "probe-create",
        )?;
        match out.exit {
            Some(5) if out.stderr.contains(DENIED) && fs::symlink_metadata(&probe).is_err() => {
                Ok(())
            }
            Some(4) => Err(Error::Sandbox(format!(
                "LEFTOVER: the sandbox allowed a create at {probe_text} and then refused its removal. Delete that file by hand"
            ))),
            _ => Err(Error::Sandbox(format!(
                "a create at {probe_text} was not denied inside the sandbox (exit {:?}, stderr {:?})",
                out.exit,
                out.stderr.trim()
            ))),
        }
    }

    /// Opening an existing file of the user's for append. The shell writes no
    /// bytes, so nothing changes if the sandbox lets it through, but a profile
    /// that denies only creation is caught. `rmdir` cannot tell the two apart
    /// because the read deny already blocks it.
    fn probe_existing_entry(&self, real_home: &Path) -> Result<()> {
        let owner = fs::metadata(real_home)
            .map_err(io_err("stat the real home"))?
            .uid();
        let mut files: Vec<PathBuf> = fs::read_dir(real_home)
            .map_err(io_err("list the real home"))?
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let meta = e.metadata().ok()?;
                let writable = meta.permissions().mode() & 0o200 != 0;
                (meta.is_file() && writable && meta.uid() == owner).then(|| e.path())
            })
            .collect();
        files.sort();
        let Some(file) = files.first() else {
            return Ok(());
        };
        let file_text = file.to_string_lossy().into_owned();
        let out = self.sandboxed(
            Path::new("/bin/sh"),
            &["-c", ": >> \"$1\"", "probe", &file_text],
            "probe-existing",
        )?;
        if out.exit == Some(0) || !out.stderr.contains(DENIED) {
            return Err(Error::Sandbox(format!(
                "opening {file_text} for writing was not denied inside the sandbox (exit {:?}, stderr {:?})",
                out.exit,
                out.stderr.trim()
            )));
        }
        Ok(())
    }

    fn tmp(&self) -> PathBuf {
        self.home
            .parent()
            .map(|r| r.join("tmp"))
            .unwrap_or_default()
    }

    pub fn version(&self) -> Result<String> {
        let out = self.exec_mo(&Invocation::Version, "version")?;
        let text = out.stdout;
        let version = text
            .lines()
            .find_map(|l| l.trim().strip_prefix("Mole version "))
            .map(|v| v.trim().to_string())
            .ok_or_else(|| Error::MoleFailed {
                what: "mo --version".into(),
                detail: format!("no `Mole version X` line in {text:?}"),
            })?;
        if SUPPORTED_MOLE.contains(&version.as_str()) {
            Ok(version)
        } else {
            Err(Error::MoleVersion { found: version })
        }
    }

    pub fn run(&self, cmd: MoleCommand) -> Result<MoleRun> {
        let argv = DryRunArgv::new(cmd);
        let mut run = self.exec_mo(&Invocation::DryRun(&argv), cmd.name())?;
        if cmd == MoleCommand::Clean {
            let list = self.home.join(".config/mole/clean-list.txt");
            run.clean_list = Some(
                fs::read_to_string(&list).map_err(io_err(format!("read {}", list.display())))?,
            );
        }
        Ok(run)
    }

    fn exec_mo(&self, inv: &Invocation<'_>, tag: &str) -> Result<MoleRun> {
        let run = self.sandboxed(&self.mo, inv.args(), tag)?;
        if run.exit != Some(0) {
            return Err(Error::MoleFailed {
                what: format!("mo {}", inv.args().join(" ")),
                detail: format!(
                    "exit {:?}\nstdout: {}\nstderr: {}",
                    run.exit,
                    tail(&run.output.stdout),
                    tail(&run.output.stderr)
                ),
            });
        }
        Ok(run.output)
    }

    fn sandboxed(&self, program: &Path, args: &[&str], tag: &str) -> Result<Sandboxed> {
        let stdout_path = self.scratch.path().join(format!("{tag}.stdout"));
        let stderr_path = self.scratch.path().join(format!("{tag}.stderr"));
        let mo_dir = self
            .mo
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut child = Command::new(SANDBOX_EXEC)
            .arg("-f")
            .arg(&self.profile)
            .arg(program)
            .args(args)
            .env_clear()
            .env("HOME", &self.home)
            .env(
                "PATH",
                format!(
                    "{}:{mo_dir}:{SYSTEM_PATH}",
                    self.scratch.path().join("shims").display()
                ),
            )
            .env("TMPDIR", self.tmp())
            .env("LANG", "en_US.UTF-8")
            .env("TERM", "dumb")
            .envs(debug_env(args))
            .current_dir(&self.home)
            .stdin(Stdio::null())
            .stdout(File::create(&stdout_path).map_err(io_err("create stdout capture"))?)
            .stderr(File::create(&stderr_path).map_err(io_err("create stderr capture"))?)
            .process_group(0)
            .spawn()
            .map_err(io_err(format!("spawn {}", program.display())))?;
        let started = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().map_err(io_err("wait for child"))? {
                break Some(status);
            }
            if interrupt::requested() {
                kill_group(child.id());
                let _ = child.wait();
                return Err(Error::Interrupted);
            }
            if started.elapsed() > self.timeout {
                kill_group(child.id());
                let _ = child.wait();
                break None;
            }
            sleep(Duration::from_millis(25));
        };
        let wall = started.elapsed();
        let read = |p: &Path| {
            fs::read(p)
                .map(|b| String::from_utf8_lossy(&b).into_owned())
                .unwrap_or_default()
        };
        let output = MoleRun {
            stdout: read(&stdout_path),
            stderr: read(&stderr_path),
            wall,
            clean_list: None,
        };
        match status {
            Some(s) => Ok(Sandboxed {
                exit: s.code(),
                output,
            }),
            None => Err(Error::MoleFailed {
                what: format!("{} {}", program.display(), args.join(" ")),
                detail: format!(
                    "timed out after {:?}\nstderr: {}",
                    self.timeout,
                    tail(&output.stderr)
                ),
            }),
        }
    }
}

struct Sandboxed {
    exit: Option<i32>,
    output: MoleRun,
}

impl std::ops::Deref for Sandboxed {
    type Target = MoleRun;
    fn deref(&self) -> &MoleRun {
        &self.output
    }
}

/// `--debug` makes Mole echo `$USER` under `set -u`, which kills it when the
/// scrubbed environment has none. A fixed name keeps the run independent of the
/// real account.
fn debug_env(args: &[&str]) -> Option<(&'static str, &'static str)> {
    args.contains(&"--debug").then_some(("USER", "dustpan"))
}

fn kill_group(pid: u32) {
    let _ = Command::new("/bin/kill")
        .args(["-KILL", "--", &format!("-{pid}")])
        .status();
}

fn tail(s: &str) -> String {
    let lines: Vec<&str> = s.lines().collect();
    lines[lines.len().saturating_sub(10)..].join("\n")
}
