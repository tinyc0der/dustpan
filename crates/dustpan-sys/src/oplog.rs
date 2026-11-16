//! [`FileStore`]: `operations.log` in the folder `paths::log_dir` names, for core's `OpLog`.
//!
//! Each append opens the file to append, writes one buffer and closes it. Two dustpan runs can
//! share the file without a lock: every line names its run and is one `write(2)` with
//! `O_APPEND`. Opening per append also means a rotation by another run is seen at once.
//! Rotation is the one step two runs could race on, so it holds an advisory lock on
//! `operations.log.lock` and reads the size again under it: two runs that start together rotate
//! once, and the second cannot replace a full `operations.log.old` with a near-empty one.
use dustpan_core::oplog::LogStore;
use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::{self, Write as _};
use std::path::PathBuf;

pub struct FileStore {
    dir: PathBuf,
}

impl FileStore {
    pub const FILE: &str = "operations.log";
    pub const OLD: &str = "operations.log.old";
    const LOCK: &str = "operations.log.lock";

    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// Opens `name` in the log folder, creating both with owner-only modes. A link at the name
    /// is refused, so the log cannot be pointed at another file (`O_NOFOLLOW`).
    fn open(&self, name: &str, options: &mut OpenOptions) -> io::Result<File> {
        let mut folder = DirBuilder::new();
        folder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::{DirBuilderExt as _, OpenOptionsExt as _};
            folder.mode(0o700);
            options
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
        }
        folder.create(&self.dir)?;
        options.create(true).open(self.dir.join(name))
    }
}

impl LogStore for FileStore {
    fn rotate_above(&mut self, limit: u64) -> io::Result<()> {
        let lock = self.open(Self::LOCK, OpenOptions::new().write(true))?;
        lock.lock()?;
        let current = self.dir.join(Self::FILE);
        let size = match fs::symlink_metadata(&current) {
            Ok(meta) => meta.len(),
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e),
        };
        if size <= limit {
            return Ok(());
        }
        // SAFE: replaces dustpan's own rotated log, a file this program created (SAFE-01).
        #[expect(
            clippy::disallowed_methods,
            reason = "SAFE-01: rotating dustpan's own log moves no user data"
        )]
        fs::rename(&current, self.dir.join(Self::OLD))
    }

    fn append(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.open(Self::FILE, OpenOptions::new().append(true))?
            .write_all(bytes)
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
        let dir = tmp.path().join("logs");
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
    fn a_link_at_the_log_path_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let elsewhere = tmp.path().join("elsewhere");
        std::fs::write(&elsewhere, b"").unwrap();
        std::os::unix::fs::symlink(&elsewhere, tmp.path().join(FileStore::FILE)).unwrap();
        let mut store = FileStore::new(tmp.path().to_path_buf());
        assert!(store.append(b"x\n").is_err());
        assert_eq!(std::fs::read(&elsewhere).unwrap(), b"");
    }
}
