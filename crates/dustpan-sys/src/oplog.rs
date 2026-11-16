//! [`FileStore`]: `operations.log` in the folder `paths::log_dir` names, for core's `OpLog`.
//!
//! The store opens the log folder once, with no link in any component, and makes every later
//! call relative to that handle, so a link at the folder or above it is refused rather than
//! followed. Each append opens the file to append, writes one buffer and closes it. Two dustpan
//! runs can share the file without a lock: every line names its run and is one `write(2)` with
//! `O_APPEND`. Opening per append also means a rotation by another run is seen at once.
//! Rotation is the one step two runs could race on, so it holds an advisory lock on
//! `operations.log.lock` and reads the size again under it: two runs that start together rotate
//! once, and the second cannot replace a full `operations.log.old` with a near-empty one.
//!
//! No call here can block the run (SAFE-22). Every open is non-blocking and must find a regular
//! file, so a FIFO at a log name is an error, and the lock is waited for at most
//! [`LOCK_WAIT`]. `OpLog` turns any error into its one warning and the run goes on.
use dustpan_core::oplog::LogStore;
use std::fs::{File, OpenOptions, TryLockError};
use std::io::{self, Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[cfg(unix)]
use unix::Folder;
#[cfg(windows)]
use windows::Folder;

pub struct FileStore {
    dir: PathBuf,
    /// Opened on first use and kept for the session.
    folder: Option<Folder>,
}

impl FileStore {
    pub const FILE: &str = "operations.log";
    pub const OLD: &str = "operations.log.old";
    pub const LOCK: &str = "operations.log.lock";

    /// The most `history` reads from one log file. Rotation keeps a file near 5 MiB, but it is
    /// checked only when a session starts, so one long session can pass that by far.
    pub const READ_CAP: u64 = 64 * 1024 * 1024;

    pub fn new(dir: PathBuf) -> Self {
        Self { dir, folder: None }
    }

    /// One log file for `history`: opened for reading only, without waiting on a FIFO, and
    /// refused when it is not a regular file or is larger than [`Self::READ_CAP`].
    pub fn read(path: &Path) -> io::Result<Vec<u8>> {
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.custom_flags(libc::O_NONBLOCK);
        }
        let mut bytes = Vec::new();
        regular(options.open(path)?)?
            .take(Self::READ_CAP + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > Self::READ_CAP {
            return Err(io::Error::new(
                io::ErrorKind::FileTooLarge,
                format!("larger than {} MiB", Self::READ_CAP / 1024 / 1024),
            ));
        }
        Ok(bytes)
    }

    fn folder(&mut self) -> io::Result<&Folder> {
        let folder = match self.folder.take() {
            Some(folder) => folder,
            None => Folder::open(&self.dir)?,
        };
        Ok(self.folder.insert(folder))
    }
}

/// A run holds the lock only to read one size and rename, so a lock held this long belongs to
/// a run that is stopped or hung.
const LOCK_WAIT: Duration = Duration::from_secs(1);

fn take_lock(lock: &File) -> io::Result<()> {
    let start = Instant::now();
    loop {
        match lock.try_lock() {
            Ok(()) => return Ok(()),
            Err(TryLockError::WouldBlock) if start.elapsed() < LOCK_WAIT => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(e) => return Err(e.into()),
        }
    }
}

fn regular(file: File) -> io::Result<File> {
    if file.metadata()?.is_file() {
        Ok(file)
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not a regular file",
        ))
    }
}

impl LogStore for FileStore {
    fn rotate_above(&mut self, limit: u64) -> io::Result<()> {
        let folder = self.folder()?;
        let lock = folder.open_file(Self::LOCK)?;
        take_lock(&lock)?;
        if folder.open_file(Self::FILE)?.metadata()?.len() > limit {
            folder.rename(Self::FILE, Self::OLD)?;
        }
        Ok(())
    }

    fn append(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.folder()?.open_file(Self::FILE)?.write_all(bytes)
    }
}

#[cfg(unix)]
mod unix {
    use crate::disk::DirHandle;
    #[cfg(target_os = "linux")]
    use crate::linux::delete::open_path;
    #[cfg(target_os = "macos")]
    use crate::macos::delete::open_path;
    use crate::unix::delete::{c_str, open_dir};
    use dustpan_core::disk::DiskError;
    use dustpan_core::path::AbsPath;
    use std::ffi::CString;
    use std::fs::File;
    use std::io;
    use std::os::fd::{AsRawFd as _, FromRawFd as _};
    use std::path::Path;

    pub(super) struct Folder(DirHandle);

    impl Folder {
        /// Opens `dir` with no link in any component, creating each missing folder owner-only
        /// inside its opened parent.
        pub(super) fn open(dir: &Path) -> io::Result<Self> {
            let dir = AbsPath::parse(dir.as_os_str())
                .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
            open_or_create(&dir).map(Self).map_err(io_error)
        }

        /// `openat` to append, creating the file owner-only. A link at the name is refused
        /// (`O_NOFOLLOW`), so the log cannot be pointed at another file, and a FIFO fails or
        /// opens at once (`O_NONBLOCK`) and is then refused as not a regular file.
        pub(super) fn open_file(&self, name: &str) -> io::Result<File> {
            let c = CString::new(name)?;
            let flags = libc::O_WRONLY
                | libc::O_APPEND
                | libc::O_CREAT
                | libc::O_NOFOLLOW
                | libc::O_NONBLOCK
                | libc::O_CLOEXEC;
            let mode: libc::c_uint = 0o600;
            // SAFETY: the folder is open and `c` is NUL-terminated.
            let fd = unsafe { libc::openat(self.0.0.as_raw_fd(), c.as_ptr(), flags, mode) };
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: `fd` was just returned by the kernel and nothing else owns it.
            super::regular(unsafe { File::from_raw_fd(fd) })
        }

        // SAFE: replaces dustpan's own rotated log, a file this program created (SAFE-01).
        #[expect(
            clippy::disallowed_methods,
            reason = "SAFE-01: rotating dustpan's own log moves no user data"
        )]
        pub(super) fn rename(&self, from: &str, to: &str) -> io::Result<()> {
            let (from, to) = (CString::new(from)?, CString::new(to)?);
            let fd = self.0.0.as_raw_fd();
            // SAFETY: the folder is open and both names are NUL-terminated.
            if unsafe { libc::renameat(fd, from.as_ptr(), fd, to.as_ptr()) } != 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }
    }

    fn open_or_create(dir: &AbsPath) -> Result<DirHandle, DiskError> {
        match open_path(dir) {
            Err(DiskError::NotFound) => {}
            opened => return opened,
        }
        let (Some(parent), Some(name)) = (dir.parent(), dir.name()) else {
            return Err(DiskError::NotFound);
        };
        let parent = open_or_create(&parent)?;
        let c = c_str(name)?;
        // A failed mkdirat shows in the open that follows, and a racing creator's EEXIST is no
        // failure.
        // SAFETY: `parent` is open and `c` is NUL-terminated.
        unsafe { libc::mkdirat(parent.0.as_raw_fd(), c.as_ptr(), 0o700) };
        open_dir(&parent, name)
    }

    /// Back to an `io::Error` for the log's warning, which names only the error's kind.
    fn io_error(e: DiskError) -> io::Error {
        match e {
            DiskError::NotFound => io::ErrorKind::NotFound.into(),
            DiskError::NotAFolder => io::ErrorKind::NotADirectory.into(),
            DiskError::Denied => io::ErrorKind::PermissionDenied.into(),
            DiskError::ReadOnly => io::ErrorKind::ReadOnlyFilesystem.into(),
            DiskError::Other(code) => io::Error::from_raw_os_error(code),
            _ => io::ErrorKind::Other.into(),
        }
    }
}

#[cfg(windows)]
mod windows {
    use std::fs::{File, OpenOptions};
    use std::io;
    use std::path::{Path, PathBuf};

    pub(super) struct Folder(PathBuf);

    impl Folder {
        pub(super) fn open(dir: &Path) -> io::Result<Self> {
            std::fs::create_dir_all(dir)?;
            Ok(Self(dir.to_path_buf()))
        }

        pub(super) fn open_file(&self, name: &str) -> io::Result<File> {
            OpenOptions::new()
                .append(true)
                .create(true)
                .open(self.0.join(name))
                .and_then(super::regular)
        }

        // SAFE: replaces dustpan's own rotated log, a file this program created (SAFE-01).
        #[expect(
            clippy::disallowed_methods,
            reason = "SAFE-01: rotating dustpan's own log moves no user data"
        )]
        pub(super) fn rename(&self, from: &str, to: &str) -> io::Result<()> {
            std::fs::rename(self.0.join(from), self.0.join(to))
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::FileStore;
    use dustpan_core::oplog::LogStore;
    use std::os::unix::fs::PermissionsExt as _;

    #[test]
    fn the_log_is_private_and_rotates_once_above_the_limit() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().canonicalize().unwrap().join("logs");
        let mut store = FileStore::new(dir.clone());
        store.append(b"0123456789\n").unwrap();
        let mode = |name: &str| {
            let meta = std::fs::metadata(dir.join(name)).unwrap();
            meta.permissions().mode() & 0o777
        };
        assert_eq!(mode(FileStore::FILE), 0o600);
        assert_eq!(
            std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
            0o700
        );
        store.rotate_above(100).unwrap();
        assert!(!dir.join(FileStore::OLD).exists(), "under the limit");
        store.rotate_above(5).unwrap();
        assert_eq!(
            std::fs::read(dir.join(FileStore::OLD)).unwrap(),
            b"0123456789\n"
        );
        store.rotate_above(5).unwrap();
        assert_eq!(
            std::fs::read(dir.join(FileStore::OLD)).unwrap(),
            b"0123456789\n",
            "a second rotation finds nothing to rotate"
        );
    }

    #[test]
    fn a_link_anywhere_in_the_log_folder_path_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        let elsewhere = root.join("elsewhere");
        std::fs::create_dir(&elsewhere).unwrap();
        std::os::unix::fs::symlink(&elsewhere, root.join("link")).unwrap();
        for dir in [root.join("link"), root.join("link/logs")] {
            let mut store = FileStore::new(dir.clone());
            assert!(store.rotate_above(0).is_err(), "{}", dir.display());
            assert!(store.append(b"x\n").is_err(), "{}", dir.display());
        }
        let left: Vec<_> = std::fs::read_dir(&elsewhere).unwrap().collect();
        assert!(left.is_empty(), "{left:?}");
    }

    #[test]
    fn a_link_at_the_log_path_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        let elsewhere = root.join("elsewhere");
        std::fs::write(&elsewhere, b"").unwrap();
        std::os::unix::fs::symlink(&elsewhere, root.join(FileStore::FILE)).unwrap();
        let mut store = FileStore::new(root);
        assert!(store.append(b"x\n").is_err());
        assert_eq!(std::fs::read(&elsewhere).unwrap(), b"");
    }
}
