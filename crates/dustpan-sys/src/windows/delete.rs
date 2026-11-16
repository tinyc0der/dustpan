//! Windows parts of the Delete sink. Every open is `NtCreateFile` of one name relative to a folder
//! handle already checked, with `FILE_OPEN_REPARSE_POINT`, so a junction, a symlink or a mount
//! point opens as itself and is never passed through. One of the files SAFE-01 lets call a
//! removal; `crates/dustpan/tests/seams.rs` lists them.
#![expect(
    clippy::disallowed_methods,
    reason = "SAFE-01: the Delete sink's disposition, reached only through core's sealed Disk calls"
)]
use super::identity::{attribute_tag, file_id, kind_of};
use crate::disk::DirHandle;
use dustpan_core::disk::{DiskError, Unlink};
use dustpan_core::identity::{DirEntry, FileId, Node, NodeKind};
use dustpan_core::path::AbsPath;
use std::ffi::{OsStr, OsString};
use std::io;
use std::os::windows::ffi::{OsStrExt as _, OsStringExt as _};
use std::os::windows::io::{AsRawHandle as _, FromRawHandle as _, OwnedHandle};
use std::path::{Component, Prefix};
use windows_sys::Wdk::Foundation::OBJECT_ATTRIBUTES;
use windows_sys::Wdk::Storage::FileSystem::{
    FILE_DIRECTORY_FILE, FILE_OPEN, FILE_OPEN_FOR_BACKUP_INTENT, FILE_OPEN_REPARSE_POINT,
    FILE_SYNCHRONOUS_IO_NONALERT, NtCreateFile,
};
use windows_sys::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_DELETE_PENDING, ERROR_DIR_NOT_EMPTY, ERROR_DIRECTORY,
    ERROR_FILE_NOT_FOUND, ERROR_INVALID_DATA, ERROR_INVALID_FUNCTION, ERROR_INVALID_NAME,
    ERROR_INVALID_PARAMETER, ERROR_LOCK_VIOLATION, ERROR_NO_MORE_FILES, ERROR_NOT_SUPPORTED,
    ERROR_PATH_NOT_FOUND, ERROR_REPARSE_POINT_ENCOUNTERED, ERROR_SHARING_VIOLATION,
    ERROR_WRITE_PROTECT, HANDLE, NTSTATUS, OBJ_CASE_INSENSITIVE, RtlNtStatusToDosError,
    STATUS_DELETE_PENDING, UNICODE_STRING,
};
use windows_sys::Win32::Storage::FileSystem::{
    DELETE, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS,
    FILE_ATTRIBUTE_RECALL_ON_OPEN, FILE_ATTRIBUTE_REPARSE_POINT, FILE_DISPOSITION_FLAG_DELETE,
    FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE, FILE_DISPOSITION_FLAG_POSIX_SEMANTICS,
    FILE_DISPOSITION_INFO, FILE_DISPOSITION_INFO_EX, FILE_FULL_DIR_INFO, FILE_LIST_DIRECTORY,
    FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_STANDARD_INFO,
    FileDispositionInfo, FileDispositionInfoEx, FileFullDirectoryInfo,
    FileFullDirectoryRestartInfo, FileStandardInfo, GetFileInformationByHandleEx, SYNCHRONIZE,
    SetFileInformationByHandle,
};
use windows_sys::Win32::System::IO::IO_STATUS_BLOCK;

/// The one Win32 error table. Core decides what each class means.
fn classify(code: u32) -> DiskError {
    match code {
        ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND | ERROR_DELETE_PENDING => DiskError::NotFound,
        ERROR_DIRECTORY | ERROR_REPARSE_POINT_ENCOUNTERED => DiskError::NotAFolder,
        ERROR_ACCESS_DENIED => DiskError::Denied,
        ERROR_WRITE_PROTECT => DiskError::ReadOnly,
        ERROR_SHARING_VIOLATION | ERROR_LOCK_VIOLATION => DiskError::Busy,
        ERROR_DIR_NOT_EMPTY => DiskError::NotEmpty,
        code => DiskError::Other(code as i32),
    }
}

/// `NtCreateFile` answers in NTSTATUS. The system maps a name whose delete is pending to "access
/// denied", but it is as good as gone.
fn classify_status(status: NTSTATUS) -> DiskError {
    if status == STATUS_DELETE_PENDING {
        return DiskError::NotFound;
    }
    // SAFETY: a table lookup on a plain integer.
    classify(unsafe { RtlNtStatusToDosError(status) })
}

fn io_error(e: &io::Error) -> DiskError {
    classify(e.raw_os_error().unwrap_or(0) as u32)
}

fn last_error() -> DiskError {
    io_error(&io::Error::last_os_error())
}

/// One name to open relative to a folder. An empty name would open the folder itself, a
/// separator would walk through whatever the folder holds under the first part, a link included,
/// and a colon would name a stream.
fn component(name: &OsStr) -> Result<Vec<u16>, DiskError> {
    let wide: Vec<u16> = name.encode_wide().collect();
    let odd = |c: &u16| *c == u16::from(b'\\') || *c == u16::from(b':');
    if wide.is_empty() || wide.iter().any(odd) {
        return Err(DiskError::Other(ERROR_INVALID_NAME as i32));
    }
    Ok(wide)
}

/// `NtCreateFile` of `name` relative to `at`, or of a whole NT path when `at` is `None`. It opens
/// the entry itself, never what a reparse point names, and shares everything, so another
/// program's open file still answers and a later delete through another handle is not blocked.
fn open(
    at: Option<&DirHandle>,
    name: &[u16],
    access: u32,
    options: u32,
) -> Result<OwnedHandle, DiskError> {
    let length = u16::try_from(size_of_val(name))
        .map_err(|_| DiskError::Other(ERROR_INVALID_NAME as i32))?;
    let object = UNICODE_STRING {
        Length: length,
        MaximumLength: length,
        Buffer: name.as_ptr().cast_mut(),
    };
    let attributes = OBJECT_ATTRIBUTES {
        Length: size_of::<OBJECT_ATTRIBUTES>() as u32,
        RootDirectory: at.map_or(std::ptr::null_mut(), |dir| dir.0.as_raw_handle()),
        ObjectName: &object,
        Attributes: OBJ_CASE_INSENSITIVE,
        SecurityDescriptor: std::ptr::null(),
        SecurityQualityOfService: std::ptr::null(),
    };
    let mut handle: HANDLE = std::ptr::null_mut();
    let mut io_status = IO_STATUS_BLOCK::default();
    // SAFETY: every pointer is valid for the call; `object` and `name` outlive it, and the system
    // only reads them.
    let status = unsafe {
        NtCreateFile(
            &mut handle,
            access | SYNCHRONIZE,
            &attributes,
            &mut io_status,
            std::ptr::null(),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            FILE_OPEN,
            options | FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
            std::ptr::null(),
            0,
        )
    };
    if status < 0 {
        return Err(classify_status(status));
    }
    // SAFETY: `NtCreateFile` succeeded, so `handle` is open and nothing else owns it.
    Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
}

/// A folder that is not a reparse point. `FILE_OPEN_REPARSE_POINT` opens a junction as the folder
/// it is, so the tag is read on the handle that would be returned.
fn folder(at: Option<&DirHandle>, name: &[u16], access: u32) -> Result<DirHandle, DiskError> {
    let handle = open(
        at,
        name,
        access | FILE_READ_ATTRIBUTES,
        FILE_DIRECTORY_FILE | FILE_OPEN_FOR_BACKUP_INTENT,
    )?;
    let tag = attribute_tag(handle.as_raw_handle()).map_err(|e| io_error(&e))?;
    if tag.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(DiskError::NotAFolder);
    }
    Ok(DirHandle(handle))
}

/// Opens `path` one component at a time from the drive root, each relative to the folder before
/// it, so a reparse point anywhere fails as `NotAFolder`. Folders on the way open to traverse
/// only; the last also opens to list.
pub(crate) fn open_path(path: &AbsPath) -> Result<DirHandle, DiskError> {
    let mut components = path.as_path().components();
    let Some(Component::Prefix(prefix)) = components.next() else {
        return Err(DiskError::NotAFolder);
    };
    let Prefix::Disk(letter) = prefix.kind() else {
        return Err(DiskError::NotAFolder);
    };
    let names: Vec<&OsStr> = components
        .filter_map(|c| match c {
            Component::Normal(name) => Some(name),
            _ => None,
        })
        .collect();
    let access = |last: bool| if last { FILE_LIST_DIRECTORY } else { 0 };
    // `\??\C:\` is the NT form of `\\?\C:\`: no name rewriting and no 260-character limit.
    let root: Vec<u16> = format!(r"\??\{}:\", char::from(letter))
        .encode_utf16()
        .collect();
    let mut dir = folder(None, &root, access(names.is_empty()))?;
    for (i, name) in names.iter().enumerate() {
        dir = folder(Some(&dir), &component(name)?, access(i + 1 == names.len()))?;
    }
    Ok(dir)
}

/// The folder `name` inside `at`, refused if it is a reparse point or not a folder.
pub(crate) fn open_dir(at: &DirHandle, name: &OsStr) -> Result<DirHandle, DiskError> {
    folder(Some(at), &component(name)?, FILE_LIST_DIRECTORY)
}

pub(crate) fn id(dir: &DirHandle) -> Result<FileId, DiskError> {
    file_id(dir.0.as_raw_handle()).map_err(|e| io_error(&e))
}

fn standard_info(handle: HANDLE) -> Result<FILE_STANDARD_INFO, DiskError> {
    let mut info = FILE_STANDARD_INFO {
        AllocationSize: 0,
        EndOfFile: 0,
        NumberOfLinks: 0,
        DeletePending: false,
        Directory: false,
    };
    // SAFETY: `info` is writable and its size is passed with it.
    let ok = unsafe {
        GetFileInformationByHandleEx(
            handle,
            FileStandardInfo,
            (&raw mut info).cast(),
            size_of::<FILE_STANDARD_INFO>() as u32,
        )
    };
    if ok == 0 {
        return Err(last_error());
    }
    Ok(info)
}

/// The entry `name` in `at`, opened as itself to read attributes only, so a locked file still
/// answers and a cloud placeholder is not recalled. A mount point is a name-surrogate reparse
/// point here: it reads as `Link` and is never entered, so `mount_root` stays false.
pub(crate) fn stat(at: &DirHandle, name: &OsStr) -> Result<DirEntry, DiskError> {
    let handle = open(Some(at), &component(name)?, FILE_READ_ATTRIBUTES, 0)?;
    let raw = handle.as_raw_handle();
    let tag = attribute_tag(raw).map_err(|e| io_error(&e))?;
    let id = file_id(raw).map_err(|e| io_error(&e))?;
    let standard = standard_info(raw)?;
    let online_only = FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS | FILE_ATTRIBUTE_RECALL_ON_OPEN;
    Ok(DirEntry {
        name: name.to_os_string(),
        node: Node {
            kind: kind_of(&tag),
            id,
        },
        alloc: u64::try_from(standard.AllocationSize).unwrap_or(0),
        links: standard.NumberOfLinks,
        mount_root: false,
        dataless: tag.FileAttributes & online_only != 0,
    })
}

/// The names in an open folder, `.` and `..` left out. The first read restarts the scan, so a
/// handle listed before starts from the top again.
fn names(dir: &DirHandle) -> Result<Vec<OsString>, DiskError> {
    // 64 KiB as `u64`s: each record starts on an 8-byte boundary and holds 8-byte fields.
    let mut buffer = vec![0u64; 8192];
    let bytes = size_of_val(buffer.as_slice());
    let name_at = std::mem::offset_of!(FILE_FULL_DIR_INFO, FileName);
    let (dot, dot_dot) = ([u16::from(b'.')], [u16::from(b'.'); 2]);
    let mut class = FileFullDirectoryRestartInfo;
    let mut out = Vec::new();
    loop {
        // SAFETY: `buffer` is writable for `bytes` bytes.
        let ok = unsafe {
            GetFileInformationByHandleEx(
                dir.0.as_raw_handle(),
                class,
                buffer.as_mut_ptr().cast(),
                bytes as u32,
            )
        };
        if ok == 0 {
            let e = io::Error::last_os_error();
            if e.raw_os_error() == Some(ERROR_NO_MORE_FILES as i32) {
                return Ok(out);
            }
            return Err(io_error(&e));
        }
        class = FileFullDirectoryInfo;
        let mut offset = 0;
        loop {
            // The records chain by `NextEntryOffset`; one that would run past the buffer is
            // refused rather than read.
            if offset + name_at > bytes {
                return Err(DiskError::Other(ERROR_INVALID_DATA as i32));
            }
            let record = buffer
                .as_ptr()
                .cast::<u8>()
                .wrapping_add(offset)
                .cast::<FILE_FULL_DIR_INFO>();
            // SAFETY: the record header lies inside `buffer`. The reads assume no alignment.
            let (next, length) = unsafe {
                (
                    (&raw const (*record).NextEntryOffset).read_unaligned(),
                    (&raw const (*record).FileNameLength).read_unaligned() as usize,
                )
            };
            if offset + name_at + length > bytes {
                return Err(DiskError::Other(ERROR_INVALID_DATA as i32));
            }
            let mut name = vec![0u16; length / 2];
            // SAFETY: the name's bytes follow the header inside `buffer`, and `name` has room for
            // them. A byte copy assumes no alignment.
            unsafe {
                std::ptr::copy_nonoverlapping(
                    (&raw const (*record).FileName).cast::<u8>(),
                    name.as_mut_ptr().cast::<u8>(),
                    name.len() * 2,
                );
            }
            if name != dot && name != dot_dot {
                out.push(OsString::from_wide(&name));
            }
            if next == 0 {
                break;
            }
            offset += next as usize;
        }
    }
}

/// Every entry of an open folder, each read with `stat` relative to it. An entry removed between
/// the read and its `stat` is left out: it is already gone.
pub(crate) fn list(
    dir: &DirHandle,
    out: &mut Vec<DirEntry>,
    stat: fn(&DirHandle, &OsStr) -> Result<DirEntry, DiskError>,
) -> Result<(), DiskError> {
    for name in names(dir)? {
        match stat(dir, &name) {
            Ok(entry) => out.push(entry),
            Err(DiskError::NotFound) => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// Opens `name` as itself with `DELETE`, checks on that handle that it is what `what` names, and
/// deletes through the same handle, so the object checked is the object deleted.
/// `Unlink::NotDir` takes a file or a link. A junction is a folder with a reparse point, and it
/// goes as the link it is: its target is not touched. `Unlink::Dir` takes only a plain folder, and
/// only an empty one.
pub(crate) fn unlink(at: &DirHandle, name: &OsStr, what: Unlink) -> Result<(), DiskError> {
    let options = match what {
        Unlink::Dir => FILE_DIRECTORY_FILE,
        Unlink::NotDir => 0,
    };
    let handle = open(
        Some(at),
        &component(name)?,
        DELETE | FILE_READ_ATTRIBUTES,
        options,
    )?;
    let raw = handle.as_raw_handle();
    let tag = attribute_tag(raw).map_err(|e| io_error(&e))?;
    let plain_folder = FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT;
    let fits = match what {
        Unlink::Dir => tag.FileAttributes & plain_folder == FILE_ATTRIBUTE_DIRECTORY,
        Unlink::NotDir => kind_of(&tag) != NodeKind::Dir,
    };
    if !fits {
        return Err(DiskError::NotAFolder);
    }
    dispose(raw)
}

/// Deletes through `handle` with POSIX semantics: the name is gone when the call returns, even
/// while another program holds the file open, and a folder with anything inside fails with
/// `ERROR_DIR_NOT_EMPTY`.
fn dispose(handle: HANDLE) -> Result<(), DiskError> {
    let posix = FILE_DISPOSITION_INFO_EX {
        Flags: FILE_DISPOSITION_FLAG_DELETE
            | FILE_DISPOSITION_FLAG_POSIX_SEMANTICS
            | FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE,
    };
    // SAFETY: `posix` is readable and its size is passed with it.
    let ok = unsafe {
        SetFileInformationByHandle(
            handle,
            FileDispositionInfoEx,
            (&raw const posix).cast(),
            size_of::<FILE_DISPOSITION_INFO_EX>() as u32,
        )
    };
    if ok != 0 {
        return Ok(());
    }
    let e = io::Error::last_os_error();
    // FAT, exFAT and older ReFS have no POSIX delete. The classic disposition removes the name
    // when the last handle closes; the caller's closes right after this returns.
    let unsupported = [
        ERROR_INVALID_PARAMETER,
        ERROR_NOT_SUPPORTED,
        ERROR_INVALID_FUNCTION,
    ];
    if !e
        .raw_os_error()
        .is_some_and(|code| unsupported.contains(&(code as u32)))
    {
        return Err(io_error(&e));
    }
    let classic = FILE_DISPOSITION_INFO { DeleteFile: true };
    // SAFETY: `classic` is readable and its size is passed with it.
    let ok = unsafe {
        SetFileInformationByHandle(
            handle,
            FileDispositionInfo,
            (&raw const classic).cast(),
            size_of::<FILE_DISPOSITION_INFO>() as u32,
        )
    };
    if ok == 0 {
        return Err(last_error());
    }
    Ok(())
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::windows::identity::node;
    use dustpan_core::probe::Seen;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    /// A temp folder under the user profile, spelled with its stored long names.
    pub(in crate::windows) fn temp() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let long = dir.path().canonicalize().unwrap();
        let plain = PathBuf::from(long.to_str().unwrap().trim_start_matches(r"\\?\"));
        (dir, plain)
    }

    pub(in crate::windows) fn abs(path: &Path) -> AbsPath {
        AbsPath::parse(path.as_os_str()).unwrap()
    }

    pub(in crate::windows) fn exists(path: &Path) -> bool {
        std::fs::symlink_metadata(path).is_ok()
    }

    /// `mklink /J`: a junction needs no privilege, unlike a symlink.
    fn junction(link: &Path, target: &Path) {
        #[expect(
            clippy::disallowed_methods,
            reason = "`mklink /J` makes the junction fixture"
        )]
        let made = Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .output()
            .unwrap();
        assert!(made.status.success(), "{made:?}");
    }

    /// The walk core's Delete sink makes: everything inside first, deepest first, then the folder.
    fn remove_tree(parent: &DirHandle, name: &str) {
        let dir = open_dir(parent, name.as_ref()).unwrap();
        let mut entries = Vec::new();
        list(&dir, &mut entries, stat).unwrap();
        for entry in entries {
            if entry.node.kind == NodeKind::Dir {
                remove_tree(&dir, entry.name.to_str().unwrap());
            } else {
                unlink(&dir, &entry.name, Unlink::NotDir).unwrap();
            }
        }
        drop(dir);
        unlink(parent, name.as_ref(), Unlink::Dir).unwrap();
    }

    #[test]
    fn a_folder_is_emptied_deepest_first_and_removed() {
        let (_dir, base) = temp();
        let tree = base.join("tree");
        std::fs::create_dir_all(tree.join(r"a\b")).unwrap();
        std::fs::write(tree.join(r"a\b\deep.txt"), b"x").unwrap();
        std::fs::write(tree.join(r"a\mid.txt"), b"x").unwrap();
        std::fs::write(tree.join("top.txt"), b"x").unwrap();
        let read_only = tree.join(r"a\b\read-only.txt");
        std::fs::write(&read_only, b"x").unwrap();
        let mut permissions = std::fs::metadata(&read_only).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&read_only, permissions).unwrap();

        let parent = open_path(&abs(&base)).unwrap();
        remove_tree(&parent, "tree");
        assert!(!exists(&tree));
    }

    #[test]
    fn a_junction_goes_as_a_link_and_its_target_keeps_its_files() {
        let (_dir, base) = temp();
        let tree = base.join("tree");
        let target = base.join("target");
        std::fs::create_dir_all(&tree).unwrap();
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("keep.txt"), b"x").unwrap();
        junction(&tree.join("link"), &target);

        let parent = open_path(&abs(&base)).unwrap();
        let inside = open_dir(&parent, "tree".as_ref()).unwrap();
        let link = OsStr::new("link");
        assert_eq!(stat(&inside, link).unwrap().node.kind, NodeKind::Link);
        assert_eq!(open_dir(&inside, link).err(), Some(DiskError::NotAFolder));
        assert_eq!(
            unlink(&inside, link, Unlink::Dir),
            Err(DiskError::NotAFolder)
        );
        drop(inside);
        remove_tree(&parent, "tree");
        assert!(!exists(&tree));
        assert!(exists(&target.join("keep.txt")));
    }

    #[test]
    fn open_path_refuses_a_junction_in_any_component() {
        let (_dir, base) = temp();
        let target = base.join("target");
        std::fs::create_dir_all(target.join("sub")).unwrap();
        junction(&base.join("link"), &target);

        assert!(open_path(&abs(&target.join("sub"))).is_ok());
        assert_eq!(
            open_path(&abs(&base.join(r"link\sub"))).err(),
            Some(DiskError::NotAFolder)
        );
        assert_eq!(
            open_path(&abs(&base.join("link"))).err(),
            Some(DiskError::NotAFolder)
        );
    }

    #[test]
    fn a_folder_with_anything_inside_stays() {
        let (_dir, base) = temp();
        let full = base.join("full");
        std::fs::create_dir_all(&full).unwrap();
        std::fs::write(full.join("file.txt"), b"x").unwrap();

        let parent = open_path(&abs(&base)).unwrap();
        let name = OsStr::new("full");
        assert_eq!(unlink(&parent, name, Unlink::Dir), Err(DiskError::NotEmpty));
        assert_eq!(
            unlink(&parent, name, Unlink::NotDir),
            Err(DiskError::NotAFolder)
        );
        assert!(exists(&full.join("file.txt")));
    }

    #[test]
    fn entries_carry_the_ids_and_kinds_the_guard_reads() {
        let (_dir, base) = temp();
        std::fs::create_dir_all(base.join("folder")).unwrap();
        std::fs::write(base.join("file.txt"), b"x").unwrap();
        junction(&base.join("link"), &base.join("folder"));

        let parent = open_path(&abs(&base)).unwrap();
        assert_eq!(
            node(&abs(&base)),
            Seen::Found(Node {
                kind: NodeKind::Dir,
                id: id(&parent).unwrap(),
            })
        );
        let mut entries = Vec::new();
        list(&parent, &mut entries, stat).unwrap();
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        let names: Vec<&OsStr> = entries.iter().map(|e| e.name.as_os_str()).collect();
        assert_eq!(names, ["file.txt", "folder", "link"]);
        for entry in &entries {
            let path = base.join(&entry.name);
            assert_eq!(node(&abs(&path)), Seen::Found(entry.node), "{entry:?}");
        }
    }
}
