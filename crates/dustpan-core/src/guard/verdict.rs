//! Verdicts as data. `explain --json` prints `layer` from [`KeptReason::layer`], `reason` from
//! [`KeptReason::code`], `detail` from `Display` and `source` from [`KeptReason::source`]; the
//! oplog (M0-5) writes the same code.
use crate::path::{AbsPath, ShapeError};
use crate::probe::{Owner, Pid, Unseen};
use crate::whitelist::{Origin, Pattern};
use std::fmt;

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Verdict {
    Eligible,
    Kept(KeptReason),
}

/// The guard layers, in the order they decide. `code` is the stable `--json` spelling.
/// `Identity` and `Contents` decide only in `Guard::recheck`, in that order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Layer {
    Shape,
    Critical,
    Live,
    Protection,
    Whitelist,
    Identity,
    Contents,
}

/// Why a path is kept. The outer variant is the layer, so a reason cannot name the wrong layer.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum KeptReason {
    Shape(ShapeReason),
    Critical(Hit),
    Live(LiveReason),
    Protected(Hit),
    Whitelisted(WhitelistHit),
    Identity(IdentityReason),
    Contents(ContentsReason),
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ShapeReason {
    Malformed(ShapeError),
    /// An ancestor is a symlink or junction. Dustpan judges and deletes physical paths only.
    LinkedAncestor {
        link: AbsPath,
    },
    /// `lstat` of this path or an ancestor gave no answer. Unknown counts as unsafe.
    Unreadable {
        at: AbsPath,
        why: Unseen,
    },
    /// The path is a link whose resolved target another layer refuses (SAFE-05, deny only).
    LinkToProtected {
        target: AbsPath,
        layer: Layer,
    },
    /// The file system stores this file under another spelling, which another layer refuses
    /// (SAFE-03, deny only). Case, case-fold, normalization and volfs aliases end here.
    AliasOfKept {
        stored: AbsPath,
        layer: Layer,
    },
    /// The path exists although its folder does not, so no ancestor of it was inspected and
    /// what it names is unknown.
    ParentNotFound {
        parent: AbsPath,
    },
    /// The path is the root of another volume than its parent's: a mount point (SAFE-38).
    /// Decided from the two `FileId`s the pass already holds; no extra syscall.
    MountPoint,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct WhitelistHit {
    pub origin: Origin,
    pub file: Option<AbsPath>,
    pub entry: Pattern,
    pub reach: Reach,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Reach {
    /// The path is what the entry names, or inside it.
    Within,
    /// The path is a folder above what the entry names (SAFE-11).
    Above,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source<'a> {
    BuiltIn,
    File(&'a AbsPath),
}

impl fmt::Display for Source<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BuiltIn => f.write_str("default"),
            Self::File(file) => file.fmt(f),
        }
    }
}

/// Refusals only `Guard::recheck` produces (SAFE-10). Discovery's identity is never refreshed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IdentityReason {
    /// Before the live probes ran, the path or its parent was no longer the object discovery saw.
    Changed,
    /// The path or its parent was replaced while the live probes ran.
    ReplacedDuringCheck,
}

/// Refusals only the survey in `Guard::recheck` produces: a folder candidate holds something
/// the guard keeps, or more than it can judge, so the whole folder stays (SAFE-27, SAFE-38).
/// Removing a folder minus a kept child would be incident I-37's parent sweep.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ContentsReason {
    /// `at`, inside the candidate, is kept for `reason`: a mount point, a file a process holds
    /// open, a protected or whitelisted name, an unreadable folder.
    Holds {
        at: AbsPath,
        reason: Box<KeptReason>,
    },
    /// `at` is an online-only placeholder. Listing it downloads it, and removing it removes the
    /// cloud copy.
    Dataless { at: AbsPath },
    /// The tree changed while the survey listed it: a folder vanished or was replaced.
    Changing { at: AbsPath },
    /// The survey stopped at a bound before it judged the whole tree (SAFE-20).
    Unbounded(Bound),
}

/// The survey's limits. A tree past one is refused before anything is removed, so the delete
/// walk never meets one halfway through (SAFE-18).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Bound {
    Entries(u64),
    /// Folders nested below the candidate. The delete walk holds one handle per level.
    Depth(usize),
    Deadline(std::time::Duration),
}

/// A policy row matched. `anchor` is the protected path, `how` says how the path relates to it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Hit {
    pub kind: Kind,
    pub anchor: AbsPath,
    pub how: How,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum How {
    Exact,
    Inside,
    ChildOf,
    /// The path, or one of its ancestors, is the same object as `anchor` under another spelling.
    SameFileAs,
    /// The path is an ancestor of `inner`, which a row protects and which exists (SAFE-27).
    Contains {
        inner: AbsPath,
    },
    /// The last component matched a protected bundle-id pattern.
    BundleId,
}

/// What a row protects. Closed, so every kind has a stable code and sentence.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    SystemPath,
    SystemFolder,
    HomeRoot,
    HomeFolder,
    StructuralFolder,
    SharedRoot,
    CloudStructure,
    ProtectedBundle,
    AppState(&'static str),
    UserData(&'static str),
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum LiveReason {
    OwnerRunning {
        owner: Owner,
        pids: Vec<Pid>,
    },
    OwnerUnknown {
        owner: Owner,
        why: Unseen,
    },
    /// A process holds open the path, a file below it, or a member of its SQLite family
    /// (SAFE-09). `file` is the one it holds, as the system spells it.
    InUse {
        pid: Pid,
        file: AbsPath,
    },
    InUseUnknown(Unseen),
    /// A `Current*` PowerLog store, live even inside the allowlisted powerlog tree (SAFE-06).
    LiveDatabase,
    /// An endpoint-security vendor's cache under `/private/var/folders`.
    EndpointSecurity,
}

impl Verdict {
    pub fn kept(&self) -> Option<&KeptReason> {
        match self {
            Self::Eligible => None,
            Self::Kept(reason) => Some(reason),
        }
    }
}

impl Layer {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Shape => "shape",
            Self::Critical => "critical",
            Self::Live => "live",
            Self::Protection => "protection",
            Self::Whitelist => "whitelist",
            Self::Identity => "identity",
            Self::Contents => "contents",
        }
    }
}

impl KeptReason {
    pub fn layer(&self) -> Layer {
        match self {
            Self::Shape(_) => Layer::Shape,
            Self::Critical(_) => Layer::Critical,
            Self::Live(_) => Layer::Live,
            Self::Protected(_) => Layer::Protection,
            Self::Whitelisted(_) => Layer::Whitelist,
            Self::Identity(_) => Layer::Identity,
            Self::Contents(_) => Layer::Contents,
        }
    }

    /// A stable code per reason (SAFE-35), shared by `--json` and the oplog.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Shape(ShapeReason::Malformed(e)) => match e {
                ShapeError::Empty => "empty-path",
                ShapeError::Relative => "relative-path",
                ShapeError::ParentComponent => "parent-component",
                ShapeError::ControlByte => "control-byte",
                ShapeError::DevicePath => "device-path",
            },
            Self::Shape(ShapeReason::LinkedAncestor { .. }) => "symlink-ancestor",
            Self::Shape(ShapeReason::Unreadable { .. }) => "unreadable",
            Self::Shape(ShapeReason::LinkToProtected { .. }) => "link-to-protected",
            Self::Shape(ShapeReason::AliasOfKept { .. }) => "alias-of-kept",
            Self::Shape(ShapeReason::ParentNotFound { .. }) => "parent-not-found",
            Self::Shape(ShapeReason::MountPoint) => "mount-point",
            Self::Critical(hit) | Self::Protected(hit) => match hit.how {
                How::Contains { .. } => "contains-protected",
                _ => hit.kind.code(),
            },
            Self::Whitelisted(hit) => match hit.reach {
                Reach::Within => "whitelisted",
                Reach::Above => "contains-whitelisted",
            },
            Self::Live(live) => match live {
                LiveReason::OwnerRunning { .. } => "owner-running",
                LiveReason::OwnerUnknown { .. } => "owner-unknown",
                LiveReason::InUse { .. } => "file-open",
                LiveReason::InUseUnknown(_) => "file-open-unknown",
                LiveReason::LiveDatabase => "live-database",
                LiveReason::EndpointSecurity => "edr-cache",
            },
            Self::Identity(IdentityReason::Changed) => "identity-changed",
            Self::Identity(IdentityReason::ReplacedDuringCheck) => "replaced-during-check",
            Self::Contents(ContentsReason::Holds { .. }) => "holds-kept",
            Self::Contents(ContentsReason::Dataless { .. }) => "holds-dataless",
            Self::Contents(ContentsReason::Changing { .. }) => "changing",
            Self::Contents(ContentsReason::Unbounded(Bound::Entries(_))) => "too-many-entries",
            Self::Contents(ContentsReason::Unbounded(Bound::Depth(_))) => "too-deep",
            Self::Contents(ContentsReason::Unbounded(Bound::Deadline(_))) => "survey-timed-out",
        }
    }

    /// For `holds-kept`, why the path inside is kept. The oplog writes its code as `inner=`, so
    /// a reader sees the cause without a second row.
    pub fn inner(&self) -> Option<&KeptReason> {
        match self {
            Self::Contents(ContentsReason::Holds { reason, .. }) => Some(reason),
            _ => None,
        }
    }

    /// Where the rule behind this refusal is written. Only a user's whitelist entry names a file.
    pub fn source(&self) -> Option<Source<'_>> {
        match self {
            Self::Whitelisted(hit) => Some(match &hit.file {
                Some(file) => Source::File(file),
                None => Source::BuiltIn,
            }),
            Self::Contents(ContentsReason::Holds { reason, .. }) => reason.source(),
            Self::Shape(_)
            | Self::Critical(_)
            | Self::Live(_)
            | Self::Protected(_)
            | Self::Identity(_)
            | Self::Contents(_) => None,
        }
    }
}

impl Kind {
    pub const fn code(self) -> &'static str {
        match self {
            Self::SystemPath => "critical-system-path",
            Self::SystemFolder => "system-folder",
            Self::HomeRoot => "home-root",
            Self::HomeFolder => "home-folder",
            Self::StructuralFolder => "structural-folder",
            Self::SharedRoot => "shared-root",
            Self::CloudStructure => "cloud-structure",
            Self::ProtectedBundle => "protected-bundle",
            Self::AppState(_) => "app-state",
            Self::UserData(_) => "user-data",
        }
    }

    fn sentence(self) -> &'static str {
        match self {
            Self::SystemPath => "critical system path",
            Self::SystemFolder => "system folder",
            Self::HomeRoot => "home folder root",
            Self::HomeFolder => "home folder",
            Self::StructuralFolder => "structural folder",
            Self::SharedRoot => "shared tool folder",
            Self::CloudStructure => "iCloud Drive structure",
            Self::ProtectedBundle => "protected app data",
            Self::AppState(label) | Self::UserData(label) => label,
        }
    }
}

impl fmt::Display for Owner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BundleId(name) | Self::Process(name) => f.write_str(name),
            Self::AppGroup(name) => write!(f, "a process of the app group {name}"),
        }
    }
}

impl fmt::Display for ShapeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Empty => "the path is empty",
            Self::Relative => "the path is not absolute",
            Self::ParentComponent => "the path contains `..`",
            Self::ControlByte => "the name contains a control character",
            Self::DevicePath => "network, device and stream paths are never cleaned",
        })
    }
}

/// The detail sentence after `Kept · `, for example `critical system path`. Each names a cause
/// and, where the user can act, what to do (SAFE-35).
impl fmt::Display for KeptReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Critical(hit) | Self::Protected(hit) => {
                f.write_str(hit.kind.sentence())?;
                match &hit.how {
                    How::Exact | How::ChildOf | How::BundleId => Ok(()),
                    How::Inside => write!(f, " (inside {})", hit.anchor),
                    How::SameFileAs => write!(f, " (same folder as {})", hit.anchor),
                    How::Contains { inner } => write!(f, " (contains {inner})"),
                }
            }
            Self::Whitelisted(hit) => hit.fmt(f),
            Self::Shape(ShapeReason::Malformed(e)) => e.fmt(f),
            Self::Shape(ShapeReason::LinkedAncestor { link }) => {
                write!(f, "symlinked ancestor {link}")
            }
            Self::Shape(ShapeReason::Unreadable { at, why }) => {
                write!(f, "cannot inspect {at}: {}", why.sentence())
            }
            Self::Shape(ShapeReason::LinkToProtected { target, .. }) => {
                write!(f, "link to a kept path {target}")
            }
            Self::Shape(ShapeReason::AliasOfKept { stored, .. }) => {
                write!(f, "another spelling of a kept path {stored}")
            }
            Self::Shape(ShapeReason::ParentNotFound { parent }) => {
                write!(
                    f,
                    "cannot tell what it names: its folder {parent} is not there"
                )
            }
            Self::Shape(ShapeReason::MountPoint) => {
                f.write_str("mount point of another volume; eject it instead")
            }
            Self::Live(LiveReason::OwnerRunning { owner, pids }) => {
                write!(f, "{owner} is running (pid")?;
                for pid in pids.iter().take(3) {
                    write!(f, " {}", pid.0)?;
                }
                f.write_str("); quit it first")
            }
            Self::Live(LiveReason::OwnerUnknown {
                owner,
                why: Unseen::Unidentified(pid),
            }) => {
                write!(
                    f,
                    "cannot tell whether {owner} is running: process {} cannot be identified; \
                     quit it first",
                    pid.0
                )
            }
            Self::Live(LiveReason::OwnerUnknown { owner, why }) => {
                write!(
                    f,
                    "cannot tell whether {owner} is running: {}",
                    why.sentence()
                )
            }
            Self::Live(LiveReason::InUse { pid, file }) => {
                write!(f, "process {} has {file} open; quit it first", pid.0)
            }
            Self::Live(LiveReason::InUseUnknown(Unseen::Unlisted(pid))) => {
                write!(
                    f,
                    "cannot tell whether a process has it open: the open files of process {} \
                     cannot be listed; quit it first",
                    pid.0
                )
            }
            Self::Live(LiveReason::InUseUnknown(why)) => {
                write!(
                    f,
                    "cannot tell whether a process has it open: {}",
                    why.sentence()
                )
            }
            Self::Live(LiveReason::LiveDatabase) => f.write_str("live power log database"),
            Self::Live(LiveReason::EndpointSecurity) => f.write_str("endpoint security cache"),
            Self::Identity(IdentityReason::Changed) => {
                f.write_str("replaced since the scan; scan again")
            }
            Self::Identity(IdentityReason::ReplacedDuringCheck) => {
                f.write_str("replaced during the final check; scan again")
            }
            Self::Contents(ContentsReason::Holds { at, reason }) => {
                write!(f, "holds {at}, which is kept: {reason}")
            }
            Self::Contents(ContentsReason::Dataless { at }) => write!(
                f,
                "holds {at}, which is online only; download it or remove it in Finder"
            ),
            Self::Contents(ContentsReason::Changing { at }) => {
                write!(
                    f,
                    "{at} changed while it was checked; quit the app using it"
                )
            }
            Self::Contents(ContentsReason::Unbounded(Bound::Entries(n))) => {
                write!(f, "holds more than {n} entries, more than one check covers")
            }
            Self::Contents(ContentsReason::Unbounded(Bound::Depth(n))) => {
                write!(f, "holds folders nested more than {n} deep")
            }
            Self::Contents(ContentsReason::Unbounded(Bound::Deadline(limit))) => write!(
                f,
                "checking what it holds took longer than {} s",
                limit.as_secs()
            ),
        }
    }
}

impl fmt::Display for Pattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Path(path) => path.fmt(f),
            Self::FinderMetadata => f.write_str("FINDER_METADATA"),
        }
    }
}

impl fmt::Display for WhitelistHit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.origin, &self.file) {
            (Origin::Default, _) => f.write_str("protected by default whitelist")?,
            (Origin::Safety, _) => f.write_str("protected by built-in safety list")?,
            (Origin::User, Some(file)) => write!(f, "protected by your whitelist {file}")?,
            (Origin::User, None) => f.write_str("protected by your whitelist")?,
        }
        match (self.reach, self.origin) {
            (Reach::Above, _) => write!(f, " (contains {})", self.entry),
            (Reach::Within, Origin::User) => write!(f, " (entry {})", self.entry),
            (Reach::Within, _) => Ok(()),
        }
    }
}

impl fmt::Display for Verdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Eligible => f.write_str("Eligible"),
            Self::Kept(reason) => write!(f, "Kept · {reason}"),
        }
    }
}
