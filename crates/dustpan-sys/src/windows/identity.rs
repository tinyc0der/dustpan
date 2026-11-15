use crate::probe::unseen;
use dustpan_core::identity::{FileId, Node, NodeKind};
use dustpan_core::path::AbsPath;
use dustpan_core::probe::Seen;
use std::io;
use std::os::windows::ffi::OsStrExt as _;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, CreateFileW, FILE_ATTRIBUTE_DIRECTORY,
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_TAG_INFO, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_ID_INFO, FILE_SHARE_DELETE, FILE_SHARE_READ,
    FILE_SHARE_WRITE, FileAttributeTagInfo, FileIdInfo, GetFileInformationByHandle,
    GetFileInformationByHandleEx, OPEN_EXISTING,
};

/// Set in the reparse tag of a symlink, a junction and every other link-like reparse point
/// (`IsReparseTagNameSurrogate`).
const NAME_SURROGATE: u32 = 0x2000_0000;

struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: the handle came from `CreateFileW` and is closed once, here.
        unsafe { CloseHandle(self.0) };
    }
}

/// Opens `path` itself, never what a link at its last component points to, and asks for no
/// access, so a locked file still answers.
fn open(path: &AbsPath) -> io::Result<Handle> {
    // The `\\?\` form lifts the 260-character limit and switches off name rewriting.
    let wide: Vec<u16> = r"\\?\"
        .encode_utf16()
        .chain(path.as_path().as_os_str().encode_wide())
        .chain([0])
        .collect();
    // SAFETY: `wide` is NUL-terminated and outlives the call; the other pointers may be null.
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        Err(io::Error::last_os_error())
    } else {
        Ok(Handle(handle))
    }
}

/// The volume serial and the 128-bit file id. `MetadataExt::file_index` is unstable on Rust
/// 1.92 and holds 64 bits, which ReFS outgrows.
fn file_id(handle: &Handle) -> io::Result<FileId> {
    let mut info = FILE_ID_INFO {
        VolumeSerialNumber: 0,
        FileId: windows_sys::Win32::Storage::FileSystem::FILE_ID_128 {
            Identifier: [0; 16],
        },
    };
    // SAFETY: `info` is writable and its size is passed with it.
    let ok = unsafe {
        GetFileInformationByHandleEx(
            handle.0,
            FileIdInfo,
            (&raw mut info).cast(),
            size_of::<FILE_ID_INFO>() as u32,
        )
    };
    if ok != 0 {
        return Ok(FileId::new(
            info.VolumeSerialNumber,
            u128::from_le_bytes(info.FileId.Identifier),
        ));
    }
    // FAT and exFAT volumes do not answer `FileIdInfo`; the 64-bit index is all they have.
    // SAFETY: a zeroed `BY_HANDLE_FILE_INFORMATION` is a valid value to overwrite.
    let mut basic: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: `basic` is writable.
    if unsafe { GetFileInformationByHandle(handle.0, &mut basic) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let index = (u64::from(basic.nFileIndexHigh) << 32) | u64::from(basic.nFileIndexLow);
    Ok(FileId::new(
        u64::from(basic.dwVolumeSerialNumber),
        u128::from(index),
    ))
}

fn kind(handle: &Handle) -> io::Result<NodeKind> {
    let mut tag = FILE_ATTRIBUTE_TAG_INFO {
        FileAttributes: 0,
        ReparseTag: 0,
    };
    // SAFETY: `tag` is writable and its size is passed with it.
    let ok = unsafe {
        GetFileInformationByHandleEx(
            handle.0,
            FileAttributeTagInfo,
            (&raw mut tag).cast(),
            size_of::<FILE_ATTRIBUTE_TAG_INFO>() as u32,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    let is_link = tag.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
        && tag.ReparseTag & NAME_SURROGATE != 0;
    Ok(if is_link {
        NodeKind::Link
    } else if tag.FileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
        NodeKind::Dir
    } else {
        NodeKind::File
    })
}

/// The Windows `lstat`: one open, two queries on the handle.
pub(crate) fn node(path: &AbsPath) -> Seen<Node> {
    let seen = open(path).and_then(|handle| {
        Ok(Node {
            kind: kind(&handle)?,
            id: file_id(&handle)?,
        })
    });
    match seen {
        Ok(node) => Seen::Found(node),
        Err(e) => unseen(&e),
    }
}
