//! The macOS Trash sink. One `renameatx_np` moves the entry from its checked parent's handle into
//! the Trash on its own volume, so what moves is the entry in the folder the guard approved,
//! wherever that folder is now. A folder on the way swapped for a link while the sink runs
//! cannot send another folder to the Trash, as a move by path (`trashItemAtURL`, spike S2's
//! first choice) could.
//!
//! The Trash is `.Trash` in the account's home for an item on the home's volume, else
//! `.Trashes/<uid>` at the root of the item's volume, as Finder uses them. It must be a folder
//! of this user that nobody else can write, with no link anywhere on the way to it
//! (`RENAME_NOFOLLOW_ANY`, SAFE-13). Privacy protection forbids opening or listing `~/.Trash`,
//! so every step there goes by name from the home's handle. The name in the Trash is picked
//! here with `RENAME_EXCL`, so the move never replaces an entry. Finder keeps no Put Back record
//! for an item moved this way. One of the files SAFE-01 lets call a move;
//! `crates/dustpan/tests/seams.rs` lists them.
#![expect(
    clippy::disallowed_methods,
    reason = "SAFE-01: the Trash sink's move, reached only through core's sealed Disk calls"
)]
use super::delete::{open_path, stat};
use crate::disk::DirHandle;
use crate::unix::delete::{c_str, classify, last_error};
use dustpan_core::disk::{DiskError, Trashed};
use dustpan_core::identity::FileId;
use dustpan_core::path::AbsPath;
use std::ffi::{CStr, OsStr, OsString};
use std::io;
use std::os::fd::AsRawFd as _;
use std::os::unix::ffi::{OsStrExt as _, OsStringExt as _};
use std::path::{Path, PathBuf};

/// From `<sys/stdio.h>` (macOS 11): fail with `ELOOP` if any component of either path is a link.
const RENAME_NOFOLLOW_ANY: libc::c_uint = 0x10;
/// From `<sys/fcntl.h>` (macOS 11): the same for `fstatat`.
const AT_SYMLINK_NOFOLLOW_ANY: libc::c_int = 0x0800;
/// How many names `name`, `name 2`, ... `name 99` the move tries before it gives up.
const NAMES: u32 = 99;

/// Moves the entry `path` names in `parent` to the Trash, after checking that it is still
/// `expect`. Never retried, never deleted (SAFE-12).
pub(crate) fn trash(
    parent: &DirHandle,
    path: &AbsPath,
    expect: FileId,
) -> Result<Trashed, DiskError> {
    let home = crate::home::account_home()
        .ok()
        .flatten()
        .ok_or(DiskError::NoTrash)?;
    trash_with_home(&home, parent, path, expect)
}

/// `trash` with the account's home given, so a test can use a fixture home.
fn trash_with_home(
    home: &Path,
    parent: &DirHandle,
    path: &AbsPath,
    expect: FileId,
) -> Result<Trashed, DiskError> {
    let name = path.name().ok_or(DiskError::NotAFolder)?;
    let bin = bin(home, parent, expect.volume())?;
    if stat(parent, name)?.node.id != expect {
        return Err(DiskError::NotAFolder);
    }
    let landed = move_in(parent, name, &bin)?;
    let inside = bin.folder.join(&landed);
    Ok(Trashed {
        from: folder_path(parent)?.join(name),
        arrived: id_at(&bin.root, &inside)?,
        at: bin.root_path.join(inside),
    })
}

/// A Trash folder, named relative to an open folder it lies in.
struct Bin {
    root: DirHandle,
    root_path: PathBuf,
    /// `.Trash` or `.Trashes/<uid>`.
    folder: PathBuf,
}

/// The home's Trash when it is on the item's volume, else the volume's own.
fn bin(home: &Path, parent: &DirHandle, volume: u64) -> Result<Bin, DiskError> {
    let home = AbsPath::parse(home.as_os_str()).map_err(|_| DiskError::NoTrash)?;
    let home_dir = open_path(&home).map_err(|_| DiskError::NoTrash)?;
    match lstat(&home_dir, Path::new(".Trash"), libc::AT_SYMLINK_NOFOLLOW) {
        Ok(st) if st.st_dev as u64 == volume => {
            mine(&st)?;
            return Ok(Bin {
                root: home_dir,
                root_path: home.as_path().to_path_buf(),
                folder: PathBuf::from(".Trash"),
            });
        }
        Ok(_) | Err(DiskError::NotFound) => {}
        Err(_) => return Err(DiskError::NoTrash),
    }
    volume_bin(parent, volume)
}

/// `.Trashes/<uid>` at the root of the item's volume, made the way Finder makes it when missing:
/// `.Trashes` writable by all with the sticky bit, and the user's folder only theirs.
fn volume_bin(parent: &DirHandle, volume: u64) -> Result<Bin, DiskError> {
    let root_path = mount_point(parent)?;
    // The system volume is read-only and has no Trash of its own.
    if root_path == Path::new("/") {
        return Err(DiskError::NoTrash);
    }
    let abs = AbsPath::parse(root_path.as_os_str()).map_err(|_| DiskError::NoTrash)?;
    let root = open_path(&abs).map_err(|_| DiskError::NoTrash)?;
    if crate::unix::delete::id(&root).map(FileId::volume) != Ok(volume) {
        return Err(DiskError::NoTrash);
    }
    let trashes = Path::new(".Trashes");
    let st = match lstat(&root, trashes, libc::AT_SYMLINK_NOFOLLOW) {
        Err(DiskError::NotFound) => make(&root, trashes, 0o1333)?,
        found => found.map_err(|_| DiskError::NoTrash)?,
    };
    let sticky_if_shared = st.st_mode & 0o002 == 0 || st.st_mode & libc::S_ISVTX != 0;
    if kind(&st) != libc::S_IFDIR || !(owned_by_me(&st) || st.st_uid == 0) || !sticky_if_shared {
        return Err(DiskError::NoTrash);
    }
    // SAFETY: geteuid takes no arguments and cannot fail.
    let folder = trashes.join(unsafe { libc::geteuid() }.to_string());
    let st = match lstat(&root, &folder, AT_SYMLINK_NOFOLLOW_ANY) {
        Err(DiskError::NotFound) => make(&root, &folder, 0o700)?,
        found => found.map_err(|_| DiskError::NoTrash)?,
    };
    mine(&st)?;
    Ok(Bin {
        root,
        root_path,
        folder,
    })
}

/// A Trash folder must be a folder of this user that nobody else can write (SAFE-13).
fn mine(st: &libc::stat) -> Result<(), DiskError> {
    let fits = kind(st) == libc::S_IFDIR && owned_by_me(st) && st.st_mode & 0o022 == 0;
    fits.then_some(()).ok_or(DiskError::NoTrash)
}

fn kind(st: &libc::stat) -> libc::mode_t {
    st.st_mode & libc::S_IFMT
}

fn owned_by_me(st: &libc::stat) -> bool {
    // SAFETY: geteuid takes no arguments and cannot fail.
    st.st_uid == unsafe { libc::geteuid() }
}

/// `mkdirat` then `fchmodat`, so the mode is exact whatever the umask, then the folder is read
/// back with no link allowed anywhere on the way.
fn make(root: &DirHandle, folder: &Path, mode: libc::mode_t) -> Result<libc::stat, DiskError> {
    let c = c_str(folder.as_os_str()).map_err(|_| DiskError::NoTrash)?;
    // SAFETY: `root` is open and `c` is NUL-terminated.
    let made = unsafe { libc::mkdirat(root.0.as_raw_fd(), c.as_ptr(), mode) } == 0;
    if !made && io::Error::last_os_error().raw_os_error() != Some(libc::EEXIST) {
        return Err(DiskError::NoTrash);
    }
    // SAFETY: as above.
    let set = unsafe { libc::fchmodat(root.0.as_raw_fd(), c.as_ptr(), mode, 0) } == 0;
    if made && !set {
        return Err(DiskError::NoTrash);
    }
    lstat(root, folder, AT_SYMLINK_NOFOLLOW_ANY).map_err(|_| DiskError::NoTrash)
}

fn lstat(at: &DirHandle, rest: &Path, flags: libc::c_int) -> Result<libc::stat, DiskError> {
    let c = c_str(rest.as_os_str())?;
    // SAFETY: a zeroed `stat` is a valid output buffer.
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    // SAFETY: `at` is open, `c` is NUL-terminated and `st` is writable.
    if unsafe { libc::fstatat(at.0.as_raw_fd(), c.as_ptr(), &mut st, flags) } != 0 {
        return Err(last_error());
    }
    Ok(st)
}

/// The id of what now sits at `rest` under `root`, with no link on the way.
fn id_at(root: &DirHandle, rest: &Path) -> Result<FileId, DiskError> {
    let st = lstat(root, rest, AT_SYMLINK_NOFOLLOW_ANY).map_err(|_| DiskError::NoTrash)?;
    // The same widening `std`'s `MetadataExt::dev` does, so ids match `Probe::node`.
    Ok(FileId::new(st.st_dev as u64, u128::from(st.st_ino)))
}

/// Moves `name` out of `parent` into the Trash under the first free name of `name`, `name 2`,
/// ... and returns the name it took.
fn move_in(parent: &DirHandle, name: &OsStr, bin: &Bin) -> Result<OsString, DiskError> {
    let from = c_str(name)?;
    for n in 1..=NAMES {
        let landed = if n == 1 {
            name.to_os_string()
        } else {
            let mut numbered = name.to_os_string();
            numbered.push(format!(" {n}"));
            numbered
        };
        let to = c_str(bin.folder.join(&landed).as_os_str())?;
        // SAFETY: both folders are open and both paths are NUL-terminated.
        let moved = unsafe {
            libc::renameatx_np(
                parent.0.as_raw_fd(),
                from.as_ptr(),
                bin.root.0.as_raw_fd(),
                to.as_ptr(),
                libc::RENAME_EXCL | RENAME_NOFOLLOW_ANY,
            )
        } == 0;
        if moved {
            return Ok(landed);
        }
        let error = io::Error::last_os_error();
        match error.raw_os_error() {
            Some(libc::EEXIST) => {}
            // A link appeared on the way to the Trash, or the Trash is on another device.
            Some(libc::ELOOP | libc::EXDEV) => return Err(DiskError::NoTrash),
            _ => return Err(refusal(&error)),
        }
    }
    Err(DiskError::NoTrash)
}

/// A move the item itself refused keeps its class (gone, denied, read-only); every other
/// failure is the Trash's, and the item stays.
fn refusal(error: &io::Error) -> DiskError {
    match classify(error) {
        e @ (DiskError::NotFound | DiskError::Denied | DiskError::ReadOnly) => e,
        _ => DiskError::NoTrash,
    }
}

/// Where the open folder is now (`F_GETPATH`), which may differ from where the guard saw it.
fn folder_path(dir: &DirHandle) -> Result<PathBuf, DiskError> {
    let mut buf = vec![0u8; libc::PATH_MAX as usize];
    // SAFETY: `dir` is open and `buf` holds `PATH_MAX` bytes, as `F_GETPATH` requires.
    if unsafe { libc::fcntl(dir.0.as_raw_fd(), libc::F_GETPATH, buf.as_mut_ptr()) } != 0 {
        return Err(last_error());
    }
    let path = CStr::from_bytes_until_nul(&buf).map_err(|_| DiskError::Other(libc::EINVAL))?;
    Ok(PathBuf::from(OsString::from_vec(path.to_bytes().to_vec())))
}

/// The mount point of the volume `dir` is on (`fstatfs`).
fn mount_point(dir: &DirHandle) -> Result<PathBuf, DiskError> {
    // SAFETY: a zeroed `statfs` is a valid output buffer.
    let mut fs: libc::statfs = unsafe { std::mem::zeroed() };
    // SAFETY: `dir` is open and `fs` is writable.
    if unsafe { libc::fstatfs(dir.0.as_raw_fd(), &mut fs) } != 0 {
        return Err(DiskError::NoTrash);
    }
    // SAFETY: `f_mntonname` is a NUL-terminated array the kernel filled.
    let on = unsafe { CStr::from_ptr(fs.f_mntonname.as_ptr()) };
    Ok(PathBuf::from(OsStr::from_bytes(on.to_bytes())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::Builder::new()
            .prefix("dustpan-trash-")
            .suffix(".noindex")
            .tempdir()
            .unwrap();
        let real = dir.path().canonicalize().unwrap();
        (dir, real)
    }

    fn item(at: &Path) -> (DirHandle, AbsPath, FileId) {
        let path = at.join("cache/com.example.item");
        std::fs::create_dir_all(path.join("d")).unwrap();
        std::fs::write(path.join("d/f"), b"cache").unwrap();
        let path = AbsPath::parse(path.as_os_str()).unwrap();
        let parent = open_path(&path.parent().unwrap()).unwrap();
        let id = stat(&parent, path.name().unwrap()).unwrap().node.id;
        (parent, path, id)
    }

    #[test]
    fn an_item_lands_in_the_homes_trash_under_a_free_name() {
        let (_dir, at) = temp();
        let home = at.join("home");
        std::fs::create_dir_all(home.join(".Trash/com.example.item")).unwrap();
        let (parent, path, id) = item(&at);
        let moved = trash_with_home(&home, &parent, &path, id).unwrap();
        assert_eq!(moved.arrived, id);
        assert_eq!(moved.at, home.join(".Trash/com.example.item 2"));
        assert_eq!(moved.from, path.as_path());
        assert!(moved.at.join("d/f").exists());
        assert!(!path.as_path().exists());
    }

    #[test]
    fn a_trash_that_is_a_link_is_refused_and_the_item_stays() {
        let (_dir, at) = temp();
        let home = at.join("home");
        let elsewhere = at.join("elsewhere");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&elsewhere).unwrap();
        std::os::unix::fs::symlink(&elsewhere, home.join(".Trash")).unwrap();
        let (parent, path, id) = item(&at);
        assert_eq!(
            trash_with_home(&home, &parent, &path, id).unwrap_err(),
            DiskError::NoTrash
        );
        assert!(path.as_path().join("d/f").exists());
        assert_eq!(std::fs::read_dir(&elsewhere).unwrap().count(), 0);
    }

    #[test]
    fn a_trash_others_can_write_is_refused() {
        use std::os::unix::fs::PermissionsExt as _;
        let (_dir, at) = temp();
        let home = at.join("home");
        std::fs::create_dir_all(home.join(".Trash")).unwrap();
        std::fs::set_permissions(home.join(".Trash"), std::fs::Permissions::from_mode(0o777))
            .unwrap();
        let (parent, path, id) = item(&at);
        assert_eq!(
            trash_with_home(&home, &parent, &path, id).unwrap_err(),
            DiskError::NoTrash
        );
        assert!(path.as_path().exists());
    }

    #[test]
    fn the_entry_in_the_approved_folder_moves_even_after_its_path_points_elsewhere() {
        let (_dir, at) = temp();
        let home = at.join("home");
        std::fs::create_dir_all(home.join(".Trash")).unwrap();
        let (parent, path, id) = item(&at);
        // The approved folder moves aside and a link to a victim takes its name.
        let victim = at.join("victim/com.example.item");
        std::fs::create_dir_all(&victim).unwrap();
        std::fs::rename(at.join("cache"), at.join("cache.aside")).unwrap();
        std::os::unix::fs::symlink(at.join("victim"), at.join("cache")).unwrap();
        let moved = trash_with_home(&home, &parent, &path, id).unwrap();
        assert_eq!(moved.arrived, id);
        assert_eq!(moved.from, at.join("cache.aside/com.example.item"));
        assert!(victim.exists(), "the victim never moves");
    }
}
