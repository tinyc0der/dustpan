//! File identity. Two spellings name the same object exactly when their `FileId`s are equal,
//! which is how the guard defeats case, Unicode-normalization, 8.3 and firmlink aliases.

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
