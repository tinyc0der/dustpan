//! The guard's verdict for a path, shared by `dustpan explain` and the Check screen. Read-only.
//! A [`Checker`] builds the guard once. Each [`Checker::check`] runs a fresh pass, so live
//! evidence is never older than the check.
use crate::config;
use dustpan_core::guard::{Guard, GuardInputs, Homes, KeptReason, ShapeReason, Verdict};
use dustpan_core::path::{AbsPath, escaped};
use dustpan_core::paths::{self, Env, Os};
use dustpan_core::probe::{self, Probe, Seen};
use dustpan_sys::LiveProbe;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// One answer. `path` is already escaped (SAFE-22), so a newline in a name shows as `\n`.
#[derive(Clone, Debug)]
pub struct Checked {
    pub path: String,
    pub verdict: Verdict,
    /// After a `symlink-ancestor` refusal: the physical spelling the user can check instead.
    pub physical: Option<AbsPath>,
}

pub struct Checker {
    guard: Guard,
    probe: LiveProbe,
    home: AbsPath,
}

impl Checker {
    /// Every error is a sentence for `dustpan: `. A guard that cannot be built judges nothing.
    pub fn open(env: Env) -> Result<Self, String> {
        let os = Os::current();
        let probe = LiveProbe::new();
        let home = paths::home_dir(os, env).and_then(|h| AbsPath::parse(h.as_os_str()).ok());
        let Some(home) = home else {
            return Err(
                "the home folder is not set to an absolute path, so no path can be judged"
                    .to_owned(),
            );
        };
        let home = physical_home(&probe, home)?;
        let account = account_home(&probe)?;
        let homes = Homes::new(home.clone(), account);
        let whitelist = config::whitelist(os, env, &homes)?;
        let inputs = GuardInputs {
            os,
            homes,
            env,
            whitelist,
        };
        let guard = Guard::new(inputs, &probe)
            .map_err(|e| format!("cannot load the protection policy: {e}"))?;
        Ok(Self { guard, probe, home })
    }

    /// The error is the working folder being gone while `arg` is relative.
    pub fn check(&self, arg: &OsStr) -> Result<Checked, String> {
        let cwd = std::env::current_dir().ok();
        let Some(typed) = absolute(arg, &self.home, cwd.as_deref()) else {
            return Err(format!(
                "cannot resolve {}: the working folder is gone; give an absolute path",
                escaped(arg)
            ));
        };
        Ok(match AbsPath::parse(typed.as_os_str()) {
            Ok(path) => {
                let judgment = self.guard.pass(&self.probe).evaluate(path);
                let physical = match judgment.verdict() {
                    Verdict::Kept(KeptReason::Shape(ShapeReason::LinkedAncestor { .. })) => {
                        physical_spelling(&self.probe, judgment.path())
                    }
                    _ => None,
                };
                Checked {
                    path: judgment.path().to_string(),
                    verdict: judgment.verdict().clone(),
                    physical,
                }
            }
            Err(e) => Checked {
                path: escaped(typed.as_os_str()).to_string(),
                verdict: Verdict::Kept(KeptReason::Shape(ShapeReason::Malformed(e))),
                physical: None,
            },
        })
    }
}

/// The path the user meant. A leading `~` is the home folder, and a relative path starts at the
/// working folder. Resolved here, at the boundary: the guard only ever sees an absolute path,
/// and `AbsPath::parse` refuses a relative one (SAFE-02).
fn absolute(arg: &OsStr, home: &AbsPath, cwd: Option<&Path>) -> Option<PathBuf> {
    let typed = Path::new(arg);
    if let Ok(rest) = typed.strip_prefix("~") {
        Some(home.as_path().join(rest))
    } else if typed.is_absolute() || arg.is_empty() {
        Some(typed.to_path_buf())
    } else {
        Some(cwd?.join(typed))
    }
}

/// The guard refuses every path below a link, so a home behind one (`/home` on Fedora Atomic) is
/// named by its physical path before any `~` rule is anchored on it. A home that does not exist
/// is used as spelled.
fn physical_home(probe: &dyn Probe, home: AbsPath) -> Result<AbsPath, String> {
    match probe::physical(probe, &home) {
        Seen::Found(physical) => Ok(physical),
        Seen::Missing => Ok(home),
        Seen::Unknown(why) => Err(format!(
            "cannot inspect the home folder {home}: {}",
            why.sentence()
        )),
    }
}

/// The account's home from the user database, physically resolved. `HOME` alone would let the
/// environment steer the guard away from the real home (I-64).
fn account_home(probe: &dyn Probe) -> Result<Option<AbsPath>, String> {
    let dir = match dustpan_sys::account_home() {
        Ok(Some(dir)) => dir,
        Ok(None) => return Ok(None),
        Err(e) => {
            return Err(format!(
                "cannot read the home folder from the user database: {e}"
            ));
        }
    };
    match AbsPath::parse(dir.as_os_str()) {
        Ok(home) => physical_home(probe, home).map(Some),
        Err(e) => Err(format!(
            "the user database names {} as the home folder: {e}",
            escaped(dir.as_os_str())
        )),
    }
}

fn physical_spelling(probe: &dyn Probe, path: &AbsPath) -> Option<AbsPath> {
    match probe::physical(probe, path) {
        Seen::Found(physical) => Some(physical),
        Seen::Missing | Seen::Unknown(_) => None,
    }
}
