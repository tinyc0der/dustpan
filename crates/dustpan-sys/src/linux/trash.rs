//! The freedesktop Trash sink, with Mole's `internal/xdgtrash` rules: the home Trash for an item
//! on its device, else `$topdir/.Trash-$uid` on the item's volume. The `.trashinfo` is reserved
//! with `O_EXCL` before the move, the move never replaces an entry and never crosses a device,
//! and a failed move removes the reserved info and leaves the item where it is (SAFE-12). One of
//! the files SAFE-01 lets call a move; `crates/dustpan/tests/seams.rs` lists them.
#![expect(
    clippy::disallowed_methods,
    reason = "SAFE-01: the Trash sink's move, reached only through core's sealed Disk calls"
)]
use super::delete::stat;
use crate::clock::SystemClock;
use crate::disk::DirHandle;
use crate::unix::delete::{c_str, classify, open_dir, owned, unlink};
use dustpan_core::disk::{DiskError, Trashed, Unlink};
use dustpan_core::identity::FileId;
use dustpan_core::path::AbsPath;
use dustpan_core::time::{Clock as _, local_stamp};
use std::ffi::{OsStr, OsString};
use std::fs::{DirBuilder, File};
use std::io::{self, Write as _};
use std::os::fd::{AsRawFd as _, FromRawFd as _};
use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::fs::{DirBuilderExt as _, MetadataExt as _};
use std::path::{Path, PathBuf};

/// Moves `path` to the invoking user's Trash after checking, relative to its open parent, that
/// the entry is still `expect`. Never retried, never deleted (SAFE-12).
pub(crate) fn trash(
    parent: &DirHandle,
    path: &AbsPath,
    expect: FileId,
) -> Result<Trashed, DiskError> {
    // SAFETY: geteuid takes no arguments and cannot fail.
    let uid = unsafe { libc::geteuid() };
    let home = std::env::var_os("HOME");
    let data_home = std::env::var_os("XDG_DATA_HOME");
    let account = account(uid, home.as_deref(), data_home.as_deref()).ok_or(DiskError::NoTrash)?;
    trash_as(&account, parent, path, expect)
}

fn trash_as(
    account: &Account,
    parent: &DirHandle,
    path: &AbsPath,
    expect: FileId,
) -> Result<Trashed, DiskError> {
    let bin = bin(path, expect.volume(), account).ok_or(DiskError::NoTrash)?;
    let name = path.name().ok_or(DiskError::NoTrash)?;
    if refused(path, account, &bin) {
        return Err(DiskError::NoTrash);
    }
    let open = open(&bin.folder, account.uid).map_err(|_| DiskError::NoTrash)?;
    let reserved = reserve(&open, name, info(&bin.recorded).as_bytes())?;
    let moved = match stat(parent, name) {
        Ok(entry) if entry.node.id == expect => rename(parent, name, &open.files, &reserved),
        Ok(_) => Err(DiskError::NotAFolder),
        Err(e) => Err(e),
    };
    if let Err(e) = moved {
        release(&open, &reserved);
        return Err(e);
    }
    let arrived = stat(&open.files, &reserved)
        .map_err(|_| DiskError::NoTrash)?
        .node
        .id;
    Ok(Trashed {
        from: folder_path(parent).join(name),
        at: bin.folder.join("files").join(&reserved),
        arrived,
    })
}

/// Where the open folder is now, from `/proc/self/fd`, which may differ from where the guard
/// saw it. The path given is the fallback when `/proc` cannot say.
fn folder_path(dir: &DirHandle) -> PathBuf {
    std::fs::read_link(format!("/proc/self/fd/{}", dir.0.as_raw_fd())).unwrap_or_default()
}

/// Who owns the Trash, and where the home Trash is.
struct Account {
    uid: libc::uid_t,
    home: PathBuf,
    /// `$XDG_DATA_HOME/Trash`, or `$HOME/.local/share/Trash`.
    trash: PathBuf,
}

/// `None` without an absolute `HOME`. A relative `XDG_DATA_HOME` is ignored, as the base
/// directory spec says.
fn account(uid: libc::uid_t, home: Option<&OsStr>, data_home: Option<&OsStr>) -> Option<Account> {
    let home = AbsPath::parse(home?).ok()?.as_path().to_path_buf();
    let data = match data_home.map(AbsPath::parse) {
        Some(Ok(data)) => data.as_path().to_path_buf(),
        _ => home.join(".local/share"),
    };
    Some(Account {
        uid,
        trash: data.join("Trash"),
        home,
    })
}

/// The Trash one item goes to.
struct Bin {
    folder: PathBuf,
    /// What `Path=` records: the item's absolute path for the home Trash, its path relative to
    /// the volume's top folder for a volume Trash.
    recorded: PathBuf,
}

/// The home Trash when the item is on its device (the device of the nearest folder on the way
/// to it that exists), else `$topdir/.Trash-$uid`, where `topdir` is the highest folder above
/// the item still on the item's device. `None` when that is `/`, or when the item's own folder
/// is on another device: the item is a mount point.
fn bin(item: &AbsPath, device: u64, account: &Account) -> Option<Bin> {
    let item = item.as_path();
    if account.trash.ancestors().find_map(device_of) == Some(device) {
        return Some(Bin {
            folder: account.trash.clone(),
            recorded: item.to_path_buf(),
        });
    }
    let top = item
        .ancestors()
        .skip(1)
        .take_while(|folder| device_of(folder) == Some(device))
        .last()
        .filter(|top| top.parent().is_some())?;
    Some(Bin {
        folder: top.join(format!(".Trash-{}", account.uid)),
        recorded: item.strip_prefix(top).ok()?.to_path_buf(),
    })
}

/// Only chooses the Trash. The move itself is between open folders, so a stale answer fails
/// with `EXDEV` rather than crossing a device.
fn device_of(path: &Path) -> Option<u64> {
    std::fs::symlink_metadata(path).ok().map(|meta| meta.dev())
}

/// `/`, the home folder, a Trash, or a folder that holds one.
fn refused(item: &AbsPath, account: &Account, bin: &Bin) -> bool {
    let item = item.as_path();
    item.parent().is_none()
        || item == account.home
        || [&account.trash, &bin.folder]
            .iter()
            .any(|trash| trash.starts_with(item) || item.starts_with(trash))
}

/// A Trash's `files` and `info`, open.
struct Open {
    files: DirHandle,
    info: DirHandle,
}

/// Opens the Trash and its `files` and `info`, creating each with mode 0700 where missing.
/// Folders above the Trash are followed, as Mole does; the three Trash folders must each be a
/// folder, not a link, owned by `uid` and not writable by group or others (SAFE-13).
fn open(folder: &Path, uid: libc::uid_t) -> Result<Open, DiskError> {
    let (Some(above), Some(name)) = (folder.parent(), folder.file_name()) else {
        return Err(DiskError::NoTrash);
    };
    DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(above)
        .map_err(|_| DiskError::NoTrash)?;
    let above_c = c_str(above.as_os_str())?;
    let flags = libc::O_PATH | libc::O_DIRECTORY | libc::O_CLOEXEC;
    // SAFETY: `above_c` is NUL-terminated.
    let above = owned(unsafe { libc::open(above_c.as_ptr(), flags) })?;
    let trash = own(&above, name, uid)?;
    Ok(Open {
        files: own(&trash, OsStr::new("files"), uid)?,
        info: own(&trash, OsStr::new("info"), uid)?,
    })
}

/// Opens `name` in `at` without following it, creating it with mode 0700 when missing, and
/// checks that only `uid` can write it.
fn own(at: &DirHandle, name: &OsStr, uid: libc::uid_t) -> Result<DirHandle, DiskError> {
    let dir = match open_dir(at, name) {
        Err(DiskError::NotFound) => {
            let c = c_str(name)?;
            // A failed mkdirat shows in the open that follows, and a racing creator's EEXIST is
            // no failure.
            // SAFETY: `at` is open and `c` is NUL-terminated.
            unsafe { libc::mkdirat(at.0.as_raw_fd(), c.as_ptr(), 0o700) };
            open_dir(at, name)?
        }
        opened => opened?,
    };
    // SAFETY: a zeroed `stat` is a valid output buffer.
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    // SAFETY: `dir` is open and `st` is writable.
    if unsafe { libc::fstat(dir.0.as_raw_fd(), &mut st) } != 0 {
        return Err(DiskError::NoTrash);
    }
    let folder = st.st_mode & libc::S_IFMT == libc::S_IFDIR;
    if folder && st.st_uid == uid && st.st_mode & 0o022 == 0 {
        Ok(dir)
    } else {
        Err(DiskError::NoTrash)
    }
}

fn info_name(name: &OsStr) -> OsString {
    let mut info = name.to_os_string();
    info.push(".trashinfo");
    info
}

fn info(recorded: &Path) -> String {
    let date = local_stamp(SystemClock::new().wall()).replacen(' ', "T", 1);
    format!(
        "[Trash Info]\nPath={}\nDeletionDate={date}\n",
        encoded(recorded.as_os_str().as_bytes())
    )
}

/// Percent-encodes every byte but the RFC 3986 unreserved ones and `/`.
fn encoded(raw: &[u8]) -> String {
    let mut out = String::with_capacity(raw.len());
    for &byte in raw {
        if byte.is_ascii_alphanumeric() || b"-._~/".contains(&byte) {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// Claims the first free name of `name`, `name.2`, ... `name.1000`: one with no entry in
/// `files`, whose `.trashinfo` this call creates with `O_EXCL` and fills with `body`, synced.
fn reserve(open: &Open, name: &OsStr, body: &[u8]) -> Result<OsString, DiskError> {
    let mode: libc::c_uint = 0o600;
    let flags = libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC;
    for n in 1..=1000 {
        let mut candidate = name.to_os_string();
        if n > 1 {
            candidate.push(format!(".{n}"));
        }
        match stat(&open.files, &candidate) {
            Ok(_) => continue,
            Err(DiskError::NotFound) => {}
            Err(_) => return Err(DiskError::NoTrash),
        }
        let c = c_str(&info_name(&candidate))?;
        // SAFETY: `open.info` is open and `c` is NUL-terminated.
        let fd = unsafe { libc::openat(open.info.0.as_raw_fd(), c.as_ptr(), flags, mode) };
        if fd < 0 {
            if io::Error::last_os_error().raw_os_error() == Some(libc::EEXIST) {
                continue;
            }
            return Err(DiskError::NoTrash);
        }
        // SAFETY: `fd` was just returned by the kernel and nothing else owns it.
        let mut file = unsafe { File::from_raw_fd(fd) };
        if file.write_all(body).and_then(|()| file.sync_all()).is_err() {
            release(open, &candidate);
            return Err(DiskError::NoTrash);
        }
        return Ok(candidate);
    }
    Err(DiskError::NoTrash)
}

/// Removes the `.trashinfo` that `reserve` created for `name`.
fn release(open: &Open, name: &OsStr) {
    let _ = unlink(&open.info, &info_name(name), Unlink::NotDir);
}

/// `renameat2(RENAME_NOREPLACE)`. On a file system without the flag, a plain `renameat` once
/// the name is checked free.
fn rename(from: &DirHandle, name: &OsStr, to: &DirHandle, target: &OsStr) -> Result<(), DiskError> {
    let (src, dst) = (c_str(name)?, c_str(target)?);
    let (from_fd, to_fd) = (from.0.as_raw_fd(), to.0.as_raw_fd());
    // SAFETY: both folders are open and both names are NUL-terminated.
    let moved = unsafe {
        libc::renameat2(
            from_fd,
            src.as_ptr(),
            to_fd,
            dst.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if moved == 0 {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    if !matches!(error.raw_os_error(), Some(libc::EINVAL | libc::ENOSYS)) {
        return Err(refusal(&error));
    }
    if !matches!(stat(to, target), Err(DiskError::NotFound)) {
        return Err(DiskError::NoTrash);
    }
    // SAFETY: both folders are open and both names are NUL-terminated.
    if unsafe { libc::renameat(from_fd, src.as_ptr(), to_fd, dst.as_ptr()) } == 0 {
        Ok(())
    } else {
        Err(refusal(&io::Error::last_os_error()))
    }
}

/// `EXDEV` lands here too: a move never copies across devices.
fn refusal(error: &io::Error) -> DiskError {
    match classify(error) {
        DiskError::Denied => DiskError::Denied,
        _ => DiskError::NoTrash,
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::{Account, account, trash_as};
    use crate::linux::delete::open_path;
    use dustpan_core::disk::{DiskError, Trashed};
    use dustpan_core::identity::FileId;
    use dustpan_core::path::AbsPath;
    use std::ffi::OsStr;
    use std::fs;
    use std::os::unix::ffi::OsStrExt as _;
    use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _, symlink};
    use std::path::{Path, PathBuf};

    /// A temp folder holding `home`, the data home `data` (so the Trash is `data/Trash`) and
    /// `items`.
    struct Fixture {
        root: tempfile::TempDir,
        account: Account,
    }

    impl Fixture {
        fn new() -> Self {
            let root = tempfile::tempdir().unwrap();
            for folder in ["home", "data", "items"] {
                fs::create_dir(root.path().join(folder)).unwrap();
            }
            let uid = fs::metadata(root.path()).unwrap().uid();
            let home = root.path().join("home");
            let data = root.path().join("data");
            let account = account(uid, Some(home.as_os_str()), Some(data.as_os_str())).unwrap();
            Self { root, account }
        }

        fn trash_folder(&self) -> PathBuf {
            self.root.path().join("data/Trash")
        }

        /// A file at `rel` under `items`.
        fn item(&self, rel: &str) -> PathBuf {
            let path = self.root.path().join("items").join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, b"bytes").unwrap();
            path
        }

        /// Trashes `path` as the sink does, approved as it is now.
        fn trash(&self, path: &Path) -> Result<Trashed, DiskError> {
            self.trash_expecting(path, id(path))
        }

        fn trash_expecting(&self, path: &Path, expect: FileId) -> Result<Trashed, DiskError> {
            let path = AbsPath::parse(path.as_os_str()).unwrap();
            let parent = open_path(&path.parent().unwrap()).unwrap();
            trash_as(&self.account, &parent, &path, expect)
        }
    }

    fn id(path: &Path) -> FileId {
        let meta = fs::symlink_metadata(path).unwrap();
        FileId::new(meta.dev(), u128::from(meta.ino()))
    }

    fn entries(folder: &Path) -> usize {
        fs::read_dir(folder).map_or(0, Iterator::count)
    }

    fn decoded(encoded: &str) -> Vec<u8> {
        let mut out = Vec::new();
        let mut rest = encoded.as_bytes();
        while let Some((&byte, tail)) = rest.split_first() {
            if byte == b'%' {
                let hex = std::str::from_utf8(&tail[..2]).unwrap();
                out.push(u8::from_str_radix(hex, 16).unwrap());
                rest = &tail[2..];
            } else {
                out.push(byte);
                rest = tail;
            }
        }
        out
    }

    /// The `Path=` and `DeletionDate=` values of a `.trashinfo`.
    fn info_lines(info: &Path) -> (String, String) {
        let text = fs::read_to_string(info).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3, "{text}");
        assert_eq!(lines[0], "[Trash Info]");
        let path = lines[1].strip_prefix("Path=").unwrap();
        let date = lines[2].strip_prefix("DeletionDate=").unwrap();
        (path.to_owned(), date.to_owned())
    }

    #[test]
    fn an_item_lands_in_files_with_a_trashinfo_naming_where_it_was() {
        let fixture = Fixture::new();
        let name = "a b%\u{fc}.txt";
        let item = fixture.item(name);
        let before = id(&item);

        let moved = fixture.trash(&item).unwrap();

        let trash = fixture.trash_folder();
        let landed = trash.join("files").join(name);
        assert_eq!(
            moved,
            Trashed {
                from: item.clone(),
                at: landed.clone(),
                arrived: before
            }
        );
        assert!(
            fs::symlink_metadata(&item).is_err(),
            "the item left its folder"
        );
        assert_eq!(fs::read(&landed).unwrap(), b"bytes");
        let (path, date) = info_lines(&trash.join("info").join(format!("{name}.trashinfo")));
        assert!(
            path.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-._~/%".contains(&b)),
            "{path}"
        );
        assert_eq!(decoded(&path), item.as_os_str().as_bytes());
        let shape: String = date
            .chars()
            .map(|c| if c.is_ascii_digit() { 'd' } else { c })
            .collect();
        assert_eq!(shape, "dddd-dd-ddTdd:dd:dd");
        for folder in [trash.clone(), trash.join("files"), trash.join("info")] {
            let mode = fs::metadata(&folder).unwrap().permissions().mode();
            assert_eq!(mode & 0o077, 0, "{} is {mode:o}", folder.display());
        }
    }

    #[test]
    fn a_second_item_with_the_same_name_gets_name_2() {
        let fixture = Fixture::new();
        let first = fixture.item("x/dup");
        let second = fixture.item("y/dup");

        fixture.trash(&first).unwrap();
        let moved = fixture.trash(&second).unwrap();

        let trash = fixture.trash_folder();
        assert_eq!(moved.at, trash.join("files/dup.2"));
        assert_eq!(moved.arrived, id(&trash.join("files/dup.2")));
        let (first_path, _) = info_lines(&trash.join("info/dup.trashinfo"));
        let (second_path, _) = info_lines(&trash.join("info/dup.2.trashinfo"));
        assert_eq!(decoded(&first_path), first.as_os_str().as_bytes());
        assert_eq!(decoded(&second_path), second.as_os_str().as_bytes());
    }

    #[test]
    fn a_trash_folder_that_is_a_link_is_refused_and_the_item_stays() {
        for link in ["Trash", "files", "info"] {
            let fixture = Fixture::new();
            let elsewhere = fixture.root.path().join("elsewhere");
            for folder in ["", "files", "info"] {
                fs::create_dir(elsewhere.join(folder)).unwrap();
            }
            let trash = fixture.trash_folder();
            if link == "Trash" {
                symlink(&elsewhere, &trash).unwrap();
            } else {
                for folder in ["", "files", "info"] {
                    if folder != link {
                        fs::create_dir(trash.join(folder)).unwrap();
                    }
                }
                symlink(elsewhere.join(link), trash.join(link)).unwrap();
            }
            let item = fixture.item("keep");
            let before = id(&item);

            assert_eq!(fixture.trash(&item), Err(DiskError::NoTrash), "{link}");
            assert_eq!(id(&item), before, "{link}");
            assert_eq!(entries(&elsewhere.join("files")), 0, "{link}");
            assert_eq!(entries(&elsewhere.join("info")), 0, "{link}");
        }
    }

    #[test]
    fn a_trash_others_can_write_is_refused() {
        for mode in [0o770, 0o707] {
            let fixture = Fixture::new();
            let trash = fixture.trash_folder();
            fs::create_dir(&trash).unwrap();
            fs::set_permissions(&trash, fs::Permissions::from_mode(mode)).unwrap();
            let item = fixture.item("keep");

            assert_eq!(fixture.trash(&item), Err(DiskError::NoTrash), "{mode:o}");
            assert!(item.exists(), "{mode:o}");
            assert_eq!(entries(&trash.join("files")), 0, "{mode:o}");
            assert_eq!(entries(&trash.join("info")), 0, "{mode:o}");
        }
    }

    #[test]
    fn a_trash_someone_else_owns_is_refused() {
        let mut fixture = Fixture::new();
        fixture.account.uid += 1;
        let item = fixture.item("keep");

        assert_eq!(fixture.trash(&item), Err(DiskError::NoTrash));
        assert!(item.exists());
        assert_eq!(entries(&fixture.trash_folder().join("files")), 0);
    }

    #[test]
    fn an_item_that_changed_stays_and_leaves_no_trashinfo() {
        let fixture = Fixture::new();
        let item = fixture.item("changed");
        let other = id(&fixture.item("other"));

        assert_eq!(
            fixture.trash_expecting(&item, other),
            Err(DiskError::NotAFolder)
        );
        assert!(item.exists());
        assert_eq!(entries(&fixture.trash_folder().join("files")), 0);
        assert_eq!(entries(&fixture.trash_folder().join("info")), 0);
    }

    #[test]
    fn the_home_folder_the_trash_and_what_holds_or_is_in_it_are_refused() {
        let fixture = Fixture::new();
        fs::create_dir_all(fixture.trash_folder().join("files/inside")).unwrap();
        for rel in ["home", "data", "data/Trash", "data/Trash/files/inside"] {
            let target = fixture.root.path().join(rel);
            let before = id(&target);

            assert_eq!(fixture.trash(&target), Err(DiskError::NoTrash), "{rel}");
            assert_eq!(id(&target), before, "{rel}");
        }
    }

    #[test]
    fn the_home_trash_is_under_the_data_home_else_under_home() {
        let trash = |home: Option<&str>, data: Option<&str>| {
            account(1, home.map(OsStr::new), data.map(OsStr::new)).map(|a| a.trash)
        };
        assert_eq!(
            trash(Some("/h"), Some("/d")),
            Some(PathBuf::from("/d/Trash"))
        );
        assert_eq!(
            trash(Some("/h/"), None),
            Some(PathBuf::from("/h/.local/share/Trash"))
        );
        assert_eq!(
            trash(Some("/h"), Some("relative")),
            Some(PathBuf::from("/h/.local/share/Trash"))
        );
        assert_eq!(trash(None, Some("/d")), None);
        assert_eq!(trash(Some("relative"), Some("/d")), None);
    }

    /// Needs a second file system. In a container:
    /// `docker run --tmpfs /vol:mode=1777 ... env DUSTPAN_TRASH_VOLUME=/vol cargo test -- --ignored`.
    #[test]
    #[ignore = "needs DUSTPAN_TRASH_VOLUME, a writable mount point on another device than the temp folder"]
    fn an_item_on_another_volume_lands_in_that_volumes_trash() {
        let volume = PathBuf::from(std::env::var_os("DUSTPAN_TRASH_VOLUME").unwrap());
        let fixture = Fixture::new();
        let folder = tempfile::tempdir_in(&volume).unwrap();
        let name = folder.path().file_name().unwrap().to_owned();
        let item = folder.path().join(&name);
        fs::write(&item, b"bytes").unwrap();
        let before = id(&item);

        let moved = fixture.trash(&item).unwrap();

        let trash = volume.join(format!(".Trash-{}", fixture.account.uid));
        assert_eq!(moved.at, trash.join("files").join(&name));
        assert_eq!(moved.arrived, before);
        let mut info = name.clone();
        info.push(".trashinfo");
        let (path, _) = info_lines(&trash.join("info").join(info));
        let relative = item.strip_prefix(&volume).unwrap();
        assert_eq!(decoded(&path), relative.as_os_str().as_bytes());
    }
}
