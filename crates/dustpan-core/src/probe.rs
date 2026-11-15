//! The read-only window core has onto the live system.
//!
//! `Probe` has five primitive questions and no policy. Core decides what to ask and what the
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

    /// The spelling the file system stores for an existing path, last component not followed.
    /// A case, case-fold, normalization or volfs alias answers with the stored name of every
    /// component, so the guard can judge the name the file really has (SAFE-03). On a
    /// case-sensitive file system the path is its own stored spelling.
    fn spelling(&self, path: &AbsPath) -> Seen<AbsPath>;

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
    /// This process runs, and the probe could not tell which app it is (SAFE-08).
    Unidentified(Pid),
}

impl Unseen {
    pub const fn sentence(self) -> &'static str {
        match self {
            Self::Denied => "permission denied",
            Self::TimedOut => "the check timed out",
            Self::Unsupported => "this build cannot check it yet",
            Self::Failed => "the check failed",
            Self::Unidentified(_) => "a running process cannot be identified",
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
    pub app: App,
}

/// Which app a process belongs to, as far as the probe could establish.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum App {
    /// The id of every bundle that encloses the executable, innermost first: a helper names its
    /// own bundle and the app that ships it. Empty for a bare executable, and on systems that
    /// have no bundles.
    Bundles(Box<[Box<str>]>),
    /// The executable is gone or its bundle cannot be read, so the process may be any app.
    /// `mine` says it runs as the user dustpan runs as; another user's process cannot own a
    /// cache in this user's home.
    Unknown { mine: bool },
}

impl App {
    /// A process with a readable executable that sits in no bundle.
    pub fn bare() -> Self {
        Self::Bundles(Box::new([]))
    }
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

/// Words that name a folder of the system layout or of a bundle, and so sit in the path of
/// unrelated software: `/Contents/MacOS/` is in the path of every bundled executable. None of
/// them is evidence of who a process is. Words under four letters (`usr`, `bin`, `opt`) never
/// count, so they are not listed. `docs/design/m0-3-guard.md` ("Fix round 2") has the counts
/// the macOS words come from; the Linux words are the FHS, snap and flatpak folders that hold
/// executables.
const LAYOUT_WORDS: &[&str] = &[
    "appex",
    "application",
    "applications",
    "bundle",
    "contents",
    "coreservices",
    "current",
    "flatpak",
    "framework",
    "frameworks",
    "games",
    "helpers",
    "home",
    "lib32",
    "lib64",
    "libexec",
    "library",
    "libx32",
    "local",
    "loginitems",
    "macos",
    "plugins",
    "private",
    "privateframeworks",
    "resources",
    "sbin",
    "share",
    "sharedsupport",
    "snap",
    "support",
    "system",
    "users",
    "utilities",
    "versions",
    "volumes",
    "xpcservices",
];

/// Whether `word`, already lowercased, can tie a command line to a bundle id.
fn is_evidence(word: &[u8]) -> bool {
    word.len() >= 4 && !LAYOUT_WORDS.iter().any(|layout| layout.as_bytes() == word)
}

/// Whether the command line of `proc` spells the bundle id `id`: the whole id, or its last
/// segment and one earlier segment as whole tokens (Mole's
/// `_mole_user_cache_owner_process_state`, lib/core/file_ops.sh:459-540). `com` is in every
/// reverse-DNS id and corroborates nothing; it is under four letters, like every other segment
/// too short to count.
fn spells(proc: &Proc, id: &str) -> bool {
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
    let counts = |word: &[u8]| is_evidence(word) && has_token(&line, word);
    counts(leaf) && segments.iter().any(|segment| counts(segment))
}

impl Owner {
    /// Whether this owner runs, from one process table. Pure, so the rule is tested with no
    /// processes.
    ///
    /// A bundle id matches a process in a bundle with that id, compared without case. A
    /// process in a bundle is that bundle's app and no other, so nothing else about it is
    /// asked. A process in no bundle, which is every process on Linux, matches when its
    /// command line spells the id ([`spells`]): the only signal there is. A process name
    /// matches an executable with exactly that file name.
    ///
    /// An owner no process matches is idle only when every process of this user was
    /// identified (SAFE-08).
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
        let mut unidentified = table
            .procs
            .iter()
            .filter(|p| matches!(p.app, App::Unknown { mine: true }));
        if !pids.is_empty() {
            OwnerState::Running(pids)
        } else if !table.complete {
            OwnerState::Unknown(Unseen::Denied)
        } else if let Some(proc) = unidentified.next() {
            OwnerState::Unknown(Unseen::Unidentified(proc.pid))
        } else {
            OwnerState::Idle
        }
    }

    fn matches(&self, proc: &Proc) -> bool {
        match self {
            Self::Process(name) => {
                Path::new(&proc.exe).file_name().and_then(|n| n.to_str()) == Some(&**name)
            }
            Self::BundleId(id) => match &proc.app {
                App::Bundles(bundles) if !bundles.is_empty() => {
                    bundles.iter().any(|b| b.eq_ignore_ascii_case(id))
                }
                App::Bundles(_) | App::Unknown { .. } => spells(proc, id),
            },
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
                    app: App::bare(),
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
    fn a_layout_word_is_no_evidence() {
        for (id, exe) in [
            (
                "com.example.macos.helper",
                "/opt/tools/Contents/MacOS/helper",
            ),
            ("com.example.system.library", "/System/Library/thing"),
            ("org.example.share.snap", "/snap/share/bin/tool"),
        ] {
            assert_eq!(
                bundle(id).state_in(&table(true, &[exe])),
                OwnerState::Idle,
                "{id}"
            );
        }
        // The same shape with a word that names a product.
        let procs = table(true, &["/opt/example/Contents/MacOS/helper"]);
        assert_eq!(
            bundle("com.example.macos.helper").state_in(&procs),
            OwnerState::Running(vec![Pid(1)])
        );
        // The whole id in the path is evidence whatever its words are.
        let procs = table(true, &["/opt/x/com.example.system.library"]);
        assert_eq!(
            bundle("com.example.system.library").state_in(&procs),
            OwnerState::Running(vec![Pid(1)])
        );
    }

    #[test]
    fn a_process_in_a_bundle_matches_by_bundle_id_alone() {
        let brave = "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser";
        let ids = App::Bundles(Box::new(["com.brave.Browser".into()]));
        let procs = in_app(table(true, &[brave]), ids);
        assert_eq!(
            bundle("com.other.brave.browser").state_in(&procs),
            OwnerState::Idle
        );
        assert_eq!(
            bundle("com.brave.browser").state_in(&procs),
            OwnerState::Running(vec![Pid(1)])
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

    fn in_app(mut table: Seen<ProcessTable>, app: App) -> Seen<ProcessTable> {
        if let Seen::Found(table) = &mut table {
            table.procs[0].app = app;
        }
        table
    }

    #[test]
    fn a_bundle_id_matches_the_bundle_a_process_runs_in() {
        // Nothing in the path names the id: only the bundle's `Info.plist` does.
        let ghostty = "/Applications/Ghostty.app/Contents/MacOS/ghostty";
        let owner = bundle("com.mitchellh.ghostty");
        assert_eq!(owner.state_in(&table(true, &[ghostty])), OwnerState::Idle);
        let ids = |ids: &[&str]| App::Bundles(ids.iter().map(|&id| id.into()).collect());
        for app in [
            ids(&["com.mitchellh.ghostty"]),
            ids(&["COM.MITCHELLH.GHOSTTY"]),
            ids(&["com.mitchellh.ghostty.helper", "com.mitchellh.ghostty"]),
        ] {
            assert_eq!(
                owner.state_in(&in_app(table(true, &[ghostty]), app)),
                OwnerState::Running(vec![Pid(1)])
            );
        }
        let other = ids(&["com.mitchellh.ghostty.helper"]);
        assert_eq!(
            owner.state_in(&in_app(table(true, &[ghostty]), other)),
            OwnerState::Idle
        );
    }

    #[test]
    fn an_unidentified_process_of_this_user_leaves_every_owner_unknown() {
        let mine = in_app(table(true, &["ghostty"]), App::Unknown { mine: true });
        for owner in [bundle("com.example.app"), Owner::Process("tool".into())] {
            assert_eq!(
                owner.state_in(&mine),
                OwnerState::Unknown(Unseen::Unidentified(Pid(1)))
            );
        }
        let theirs = in_app(table(true, &["ghostty"]), App::Unknown { mine: false });
        assert_eq!(
            bundle("com.example.app").state_in(&theirs),
            OwnerState::Idle
        );
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
