//! File identity. Two spellings name the same object exactly when their `FileId`s are equal,
//! which is how the guard knows an alias of a protected anchor, however it is spelled. A path
//! that no anchor names is judged again under the spelling the file system stores for it
//! (`Probe::spelling`).
use std::ffi::OsString;

/// Device and inode on Unix. Volume serial and the 128-bit `FILE_ID_INFO` id on Windows, because
/// ReFS (Dev Drive, where build caches live) needs all 128 bits to stay unique.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct FileId {
    volume: u64,
    file: u128,
}

impl FileId {
    pub const fn new(volume: u64, file: u128) -> Self {
        Self { volume, file }
    }

    /// A child on another volume than its parent is a mount point (SAFE-38).
    pub const fn volume(self) -> u64 {
        self.volume
    }

    /// The inode, or the file id within the volume.
    pub const fn file(self) -> u128 {
        self.file
    }
}

/// What `lstat` saw. The last component is never followed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Node {
    pub kind: NodeKind,
    pub id: FileId,
}

/// `Link` covers symlinks, and junctions and other name-surrogate reparse points on Windows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NodeKind {
    Dir,
    File,
    Link,
    Other,
}

/// The object a candidate named at discovery, and the directory that held it (SAFE-10).
///
/// The physical parent path is the candidate path's parent: the guard refuses any path with a
/// linked ancestor, so a candidate path is always physical. No mtime: a directory's mtime moves
/// whenever an entry inside it changes, so an mtime-bound identity would refuse every cache a
/// daemon touched while the user read the review screen. Content age is SAFE-26's job (M3).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct PathIdentity {
    pub parent: FileId,
    pub target: FileId,
}

/// One entry of a directory read. The survey in `Guard::recheck` judges each one, and the delete
/// walk reads `alloc` and `links` to count freed bytes (SAFE-21).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DirEntry {
    /// The name as the directory stores it. It may hold a control byte, which `path::Name`
    /// refuses: the guard judges such an entry rather than keep the whole folder for it, and
    /// every printed form escapes it (SAFE-22).
    pub name: OsString,
    pub node: Node,
    /// Allocated bytes, not logical length (SAFE-21).
    pub alloc: u64,
    /// Hard links to the object. Its bytes are freed only when the last one goes.
    pub links: u32,
    /// The root of a mount. True for a Linux bind mount, which shares its parent's device and so
    /// escapes the `FileId::volume` comparison (SAFE-38).
    pub mount_root: bool,
    /// An online-only placeholder (macOS `SF_DATALESS`). Listing or removing it reaches the
    /// cloud copy, so the survey keeps the candidate (SAFE-38).
    pub dataless: bool,
}
