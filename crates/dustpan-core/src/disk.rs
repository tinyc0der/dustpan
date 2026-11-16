//! The file operations a sink needs, relative to an open folder handle.
//!
//! `dustpan-sys` implements [`Disk`] per OS (`openat`, `fstat`, `unlinkat` on Unix,
//! `NtCreateFile` relative to a folder handle on Windows), and `probe::fake::FakeProbe`
//! implements it in memory. Every method takes a [`Seal`], which only `dustpan-core` can build,
//! so nothing outside core can call one: the binary, the TUI and a later milestone's planner can
//! hold a `LiveDisk` and still cannot remove a file except by handing an
//! [`crate::guard::Approved`] to [`crate::sink::Executor`] in apply mode (SAFE-01). The
//! workspace `clippy.toml` bans the raw calls everywhere in sys except the files that implement
//! this trait.
//!
//! The caller cannot reach a method without a seal:
//! ```compile_fail,E0451
//! fn delete(disk: &impl dustpan_core::disk::Disk, path: &dustpan_core::path::AbsPath) {
//!     let _ = disk.open_parent(path, &dustpan_core::disk::Seal(()));
//! }
//! ```
use crate::identity::{DirEntry, FileId};
use crate::path::AbsPath;
use std::ffi::OsStr;
use std::path::PathBuf;

/// Proof that a [`Disk`] call comes from core's sink code. The private field means only this
/// crate builds one. An implementation receives it and never needs to build one.
pub struct Seal(());

impl Seal {
    pub(crate) const fn new() -> Self {
        Self(())
    }
}

/// What `unlinkat` is asked to remove: `AT_REMOVEDIR` or not.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Unlink {
    NotDir,
    /// Removes only an empty folder, so a folder swapped in under the name loses nothing.
    Dir,
}

/// An OS error, classified once in sys so core decides on meaning, not on errno values.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DiskError {
    /// `ENOENT`. The sink counts a missing target as already gone.
    NotFound,
    /// A link, or anything but a folder, where the walk needed a folder (`ELOOP`, `ENOTDIR`, a
    /// reparse point): the path no longer resolves to what the guard approved.
    NotAFolder,
    /// `EACCES`, `EPERM`, or Foundation's `NSFileWriteNoPermissionError` (a container folder
    /// `containermanagerd` owns, per spike S2). Never retried as anything else.
    Denied,
    /// `EROFS`.
    ReadOnly,
    /// `EBUSY`: a mount point, or an open file on Windows.
    Busy,
    /// `ENOTEMPTY`: something new is inside a folder the walk emptied.
    NotEmpty,
    /// No Trash can take the item: no Trash on its volume, a Trash folder that is a link or
    /// someone else's (SAFE-13), or a volume with no Recycle Bin.
    NoTrash,
    /// A dataless placeholder the walk refused to materialize.
    Dataless,
    Other(i32),
}

/// What a Trash move moved and where it put it, for "Reveal in Trash". A move on one volume
/// keeps the `FileId`, so core compares `arrived` with the approved target. `from` is where the
/// moved entry was at the moment of the move, which differs from the candidate's path when its
/// folder moved meanwhile.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Trashed {
    pub from: PathBuf,
    pub at: PathBuf,
    pub arrived: FileId,
}

/// File operations relative to an open folder. `Dir` is an owned handle that closes on drop.
///
/// The parent of a candidate is opened with no link in any component, then every step below it
/// is `open_dir` from a handle already checked, so no step resolves a path from the root again.
pub trait Disk {
    type Dir;

    /// Opens the folder that holds `path`, refusing a link in any component: `open(2)` with
    /// `O_NOFOLLOW_ANY | O_DIRECTORY` on macOS, `openat` per component with `O_NOFOLLOW` on
    /// Linux, a per-component open that refuses a reparse point on Windows.
    fn open_parent(&self, path: &AbsPath, seal: &Seal) -> Result<Self::Dir, DiskError>;

    /// Opens the folder `name` inside `at` without following it
    /// (`openat` with `O_NOFOLLOW | O_DIRECTORY`).
    fn open_dir(&self, at: &Self::Dir, name: &OsStr, seal: &Seal) -> Result<Self::Dir, DiskError>;

    /// `fstat` of an open folder.
    fn id(&self, dir: &Self::Dir, seal: &Seal) -> Result<FileId, DiskError>;

    /// `fstatat(AT_SYMLINK_NOFOLLOW)` of one entry.
    fn stat(&self, at: &Self::Dir, name: &OsStr, seal: &Seal) -> Result<DirEntry, DiskError>;

    /// Every entry of an open folder, without materializing a dataless one.
    fn list(&self, dir: &Self::Dir, out: &mut Vec<DirEntry>, seal: &Seal) -> Result<(), DiskError>;

    /// `unlinkat` relative to `at`. On Windows the entry is opened relative to `at` and
    /// deleted through its own handle with POSIX semantics.
    fn unlink(
        &self,
        at: &Self::Dir,
        name: &OsStr,
        what: Unlink,
        seal: &Seal,
    ) -> Result<(), DiskError>;

    /// Moves `path`, whose folder is `at`, to the invoking user's Trash, after checking with
    /// `fstatat` that the entry is still `expect`, and reports the `FileId` that arrived. macOS:
    /// `renameatx_np` from `at` into the home or volume Trash. Linux: the freedesktop home or
    /// volume Trash.
    /// Windows: the Recycle Bin through `IFileOperation`, refusing a permanent delete. Never
    /// falls back to removal (SAFE-12).
    fn trash(
        &self,
        at: &Self::Dir,
        path: &AbsPath,
        expect: FileId,
        seal: &Seal,
    ) -> Result<Trashed, DiskError>;
}
