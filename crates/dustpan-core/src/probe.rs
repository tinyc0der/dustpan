//! The read-only window core has onto the live system.
//!
//! `Probe` has four primitive questions and no policy. Core decides what to ask and what the
//! answers mean, so every rule that depends on them runs against `fake::FakeProbe` in a unit
//! test. An implementation keeps nothing between calls: each answer is a fresh observation.
//! Sharing answers is the job of a `guard::Pass`, which is visible, bounded and never reaches
//! the sink.
use crate::identity::{Node, NodeKind};
use crate::path::AbsPath;
use std::collections::VecDeque;
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf, Prefix};

#[cfg(any(test, feature = "testkit"))]
pub mod fake;

pub trait Probe: Sync {
    /// One `lstat`. Never follows the last component.
    fn node(&self, path: &AbsPath) -> Seen<Node>;

    /// One `readlink` of a path that [`Probe::node`] reported as `NodeKind::Link`.
    /// The target is raw: relative targets are resolved by core.
    fn link_target(&self, link: &AbsPath) -> Seen<PathBuf>;

    /// One snapshot of the process table. Owner matching happens in core
    /// ([`Owner::state_in`]), so the rule that says who owns a cache is tested without processes.
    fn processes(&self) -> Seen<ProcessTable>;

    /// Whether any process holds any of `files` open, asked once for the whole set so a SQLite
    /// database and its `-wal`, `-shm` and `-journal` companions are judged as one unit (SAFE-09).
    fn in_use(&self, files: &[AbsPath]) -> Handles;
}

/// The tri-state every probe answer takes. `Missing` is a conclusive "does not exist".
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Seen<T> {
    Found(T),
    Missing,
    Unknown(Unseen),
}

/// Why a probe could not answer. Carried into the refusal, so `explain` names a cause (SAFE-35).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Unseen {
    Denied,
    TimedOut,
    /// This build has no implementation yet (for example `in_use` before M3).
    Unsupported,
    Failed,
}

impl Unseen {
    pub const fn sentence(self) -> &'static str {
        match self {
            Self::Denied => "permission denied",
            Self::TimedOut => "the check timed out",
            Self::Unsupported => "this build cannot check it yet",
            Self::Failed => "the check failed",
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ProcessTable {
    /// The probe saw every process, including root's. An owner absent from an incomplete table
    /// is `Unknown`, never `Idle` (SAFE-08).
    pub complete: bool,
    pub procs: Vec<Proc>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Proc {
    pub pid: Pid,
    /// The executable's path.
    pub exe: OsString,
    /// The arguments after the executable, joined by spaces, where the OS exposes them.
    pub args: OsString,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Pid(pub u32);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Handles {
    Closed,
    Open,
    Unknown(Unseen),
}

/// Who may hold a candidate open. Names are literal: no pattern syntax reaches a matcher (SAFE-33).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Owner {
    /// A reverse-DNS id such as `com.google.Chrome`, from a rule or from a cache folder's name.
    BundleId(Box<str>),
    /// An executable name such as `Google Chrome`.
    Process(Box<str>),
}

/// PLAN's `OwnerState`, with the cause kept on `Unknown`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum OwnerState {
    Idle,
    Running(Vec<Pid>),
    Unknown(Unseen),
}

impl OwnerState {
    /// The one translation of the tri-state (SAFE-08). Only a conclusive `Idle` permits removal.
    pub fn permits_removal(&self) -> bool {
        matches!(self, Self::Idle)
    }
}

const MAX_LINK_HOPS: usize = 40;

/// The physical spelling of `path`: every link, including the last component, resolved through
/// [`Probe::node`] and [`Probe::link_target`], at most 40 hops. Used for a leaf link's target
/// (SAFE-05), for protected anchors when a guard is built, and by `explain` to suggest the
/// physical path after a `symlink-ancestor` refusal. Resolution lives in core so the fake and
/// the real probe share it, and a resolved spelling can only add refusals.
pub fn physical(probe: &dyn Probe, path: &AbsPath) -> Seen<AbsPath> {
    let mut pending: VecDeque<OsString> = VecDeque::new();
    let mut cur = PathBuf::new();
    if !restart(path.as_path(), &mut cur, &mut pending) {
        return Seen::Unknown(Unseen::Failed);
    }
    let mut hops = 0;
    while let Some(name) = pending.pop_front() {
        if name == ".." {
            // `cur` holds no link, so its parent is the physical parent.
            cur.pop();
            continue;
        }
        let next = AbsPath::trusted(cur.join(&name));
        match probe.node(&next) {
            Seen::Found(node) if node.kind == NodeKind::Link => {
                hops += 1;
                if hops > MAX_LINK_HOPS {
                    return Seen::Unknown(Unseen::Failed);
                }
                let target = match probe.link_target(&next) {
                    Seen::Found(target) => target,
                    Seen::Missing => return Seen::Missing,
                    Seen::Unknown(why) => return Seen::Unknown(why),
                };
                if !restart(&target, &mut cur, &mut pending) {
                    return Seen::Unknown(Unseen::Failed);
                }
            }
            Seen::Found(_) => cur = next.as_path().to_path_buf(),
            Seen::Missing => return Seen::Missing,
            Seen::Unknown(why) => return Seen::Unknown(why),
        }
    }
    match AbsPath::parse(cur.as_os_str()) {
        Ok(resolved) => Seen::Found(resolved),
        Err(_) => Seen::Unknown(Unseen::Failed),
    }
}

/// Queues `target`'s components in front of `pending`. An absolute target replaces `cur` with its
/// root. False for a target on a UNC or device prefix, which dustpan never follows.
fn restart(target: &Path, cur: &mut PathBuf, pending: &mut VecDeque<OsString>) -> bool {
    let mut names = Vec::new();
    for component in target.components() {
        match component {
            Component::Prefix(p) => match p.kind() {
                Prefix::Disk(_) => *cur = PathBuf::from(component.as_os_str()),
                Prefix::VerbatimDisk(letter) => {
                    *cur = PathBuf::from(format!("{}:", char::from(letter)));
                }
                _ => return false,
            },
            Component::RootDir => {
                // A rooted target keeps the drive and leaves the folder the link sat in.
                let drive: PathBuf = cur
                    .components()
                    .take_while(|c| matches!(c, Component::Prefix(_)))
                    .collect();
                *cur = drive;
                cur.push(component.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => names.push(OsString::from("..")),
            Component::Normal(name) => names.push(name.to_os_string()),
        }
    }
    for name in names.into_iter().rev() {
        pending.push_front(name);
    }
    true
}

fn lower(bytes: &[u8]) -> Vec<u8> {
    bytes.to_ascii_lowercase()
}

/// Every start of `needle` in `hay`. Both are already lowercased.
fn finds<'a>(hay: &'a [u8], needle: &'a [u8]) -> impl Iterator<Item = usize> + 'a {
    let last = hay.len().checked_sub(needle.len());
    (0..=last.unwrap_or(0))
        .filter(move |&at| last.is_some() && !needle.is_empty() && hay[at..].starts_with(needle))
}

/// True when `needle` occurs with no letter or digit on either side.
fn has_token(hay: &[u8], needle: &[u8]) -> bool {
    finds(hay, needle).any(|at| {
        let before = at.checked_sub(1).map(|i| hay[i]);
        let after = hay.get(at + needle.len()).copied();
        !before.is_some_and(|b| b.is_ascii_alphanumeric())
            && !after.is_some_and(|b| b.is_ascii_alphanumeric())
    })
}

impl Owner {
    /// Whether this owner runs, from one process table. Pure, so the rule is tested with no
    /// processes. A bundle id matches a process whose command line contains the id, or contains
    /// the id's last segment and one earlier segment as whole tokens (Mole's
    /// `_mole_user_cache_owner_process_state`, lib/core/file_ops.sh:459-540). A process name
    /// matches an executable with exactly that file name.
    pub fn state_in(&self, table: &Seen<ProcessTable>) -> OwnerState {
        let table = match table {
            Seen::Found(table) => table,
            Seen::Missing => return OwnerState::Unknown(Unseen::Failed),
            Seen::Unknown(why) => return OwnerState::Unknown(*why),
        };
        let pids: Vec<Pid> = table
            .procs
            .iter()
            .filter(|p| self.matches(p))
            .map(|p| p.pid)
            .collect();
        if !pids.is_empty() {
            OwnerState::Running(pids)
        } else if table.complete {
            OwnerState::Idle
        } else {
            OwnerState::Unknown(Unseen::Denied)
        }
    }

    fn matches(&self, proc: &Proc) -> bool {
        match self {
            Self::Process(name) => {
                Path::new(&proc.exe).file_name().and_then(|n| n.to_str()) == Some(&**name)
            }
            Self::BundleId(id) => {
                let mut line = lower(proc.exe.as_encoded_bytes());
                line.push(b' ');
                line.extend(lower(proc.args.as_encoded_bytes()));
                let id = lower(id.as_bytes());
                if finds(&line, &id).next().is_some() {
                    return true;
                }
                let mut segments: Vec<&[u8]> = id.split(|&b| b == b'.').collect();
                let Some(leaf) = segments.pop() else {
                    return false;
                };
                if segments.is_empty() || leaf.len() < 4 || !has_token(&line, leaf) {
                    return false;
                }
                // `com` is in every reverse-DNS id and corroborates nothing; it is under four
                // bytes, like every other segment too short to count.
                segments.iter().any(|s| s.len() >= 4 && has_token(&line, s))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(complete: bool, exes: &[&str]) -> Seen<ProcessTable> {
        Seen::Found(ProcessTable {
            complete,
            procs: exes
                .iter()
                .enumerate()
                .map(|(i, exe)| Proc {
                    pid: Pid(i as u32 + 1),
                    exe: OsString::from(exe),
                    args: OsString::new(),
                })
                .collect(),
        })
    }

    fn bundle(id: &str) -> Owner {
        Owner::BundleId(id.into())
    }

    #[test]
    fn a_bundle_id_in_the_command_line_means_running() {
        let procs = table(
            true,
            &["/Applications/X.app/Contents/MacOS/COM.EXAMPLE.app helper"],
        );
        assert_eq!(
            bundle("com.example.app").state_in(&procs),
            OwnerState::Running(vec![Pid(1)])
        );
    }

    #[test]
    fn the_leaf_needs_one_corroborating_segment_on_token_boundaries() {
        let chrome = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
        assert!(matches!(
            bundle("com.google.Chrome").state_in(&table(true, &[chrome])),
            OwnerState::Running(_)
        ));
        let other = "/Applications/Chromecast.app/Contents/MacOS/Chromecast";
        assert_eq!(
            bundle("com.google.Chrome").state_in(&table(true, &[other])),
            OwnerState::Idle
        );
        let leaf_only = "/usr/local/bin/chrome";
        assert_eq!(
            bundle("com.google.Chrome").state_in(&table(true, &[leaf_only])),
            OwnerState::Idle
        );
    }

    #[test]
    fn only_a_complete_table_proves_an_owner_idle() {
        let owner = bundle("com.example.app");
        assert_eq!(
            owner.state_in(&table(true, &["/bin/zsh"])),
            OwnerState::Idle
        );
        assert_eq!(
            owner.state_in(&table(false, &["/bin/zsh"])),
            OwnerState::Unknown(Unseen::Denied)
        );
        assert_eq!(
            owner.state_in(&Seen::Unknown(Unseen::TimedOut)),
            OwnerState::Unknown(Unseen::TimedOut)
        );
        assert!(!OwnerState::Unknown(Unseen::Denied).permits_removal());
        assert!(!OwnerState::Running(vec![Pid(1)]).permits_removal());
        assert!(OwnerState::Idle.permits_removal());
    }

    #[test]
    fn a_process_name_is_literal() {
        let procs = table(true, &["/usr/bin/axxb", "/usr/bin/a.*b"]);
        assert_eq!(
            Owner::Process("a.*b".into()).state_in(&procs),
            OwnerState::Running(vec![Pid(2)])
        );
    }
}
