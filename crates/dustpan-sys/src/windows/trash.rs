//! The Windows Trash sink: the Recycle Bin through `IFileOperation`, in a single-threaded COM
//! apartment on the calling thread. Under `FOF_NO_UI` the shell deletes for good, without asking,
//! an item it cannot recycle (too large for the bin, or a bin turned off for the drive), so a
//! progress sink refuses every delete the shell will not recycle (SAFE-12). One of the files
//! SAFE-01 lets call a move; `crates/dustpan/tests/seams.rs` lists them.
#![expect(
    clippy::disallowed_methods,
    reason = "SAFE-01: the Trash sink's recycle, reached only through core's sealed Disk calls"
)]
use super::delete::stat;
use super::identity::node;
use crate::disk::DirHandle;
use dustpan_core::disk::{DiskError, Trashed};
use dustpan_core::identity::FileId;
use dustpan_core::path::AbsPath;
use dustpan_core::probe::Seen;
use std::ffi::OsString;
use std::os::windows::ffi::{OsStrExt as _, OsStringExt as _};
use std::path::{Component, PathBuf};
use std::sync::Mutex;
use windows::Win32::Foundation::{E_ABORT, E_ACCESSDENIED};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoCreateInstance, CoInitializeEx,
    CoTaskMemFree, CoUninitialize,
};
use windows::Win32::UI::Shell::{
    COPYENGINE_E_ACCESS_DENIED_SRC, FOF_ALLOWUNDO, FOF_NO_UI, FOFX_EARLYFAILURE,
    FOFX_RECYCLEONDELETE, FileOperation, IFileOperation, IFileOperationProgressSink,
    IFileOperationProgressSink_Impl, IShellItem, SHCreateItemFromParsingName, SIGDN_FILESYSPATH,
    TSF_DELETE_RECYCLE_IF_POSSIBLE,
};
use windows::core::{ComObject, HRESULT, PCWSTR, Ref, implement};
use windows_sys::Win32::Storage::FileSystem::GetDriveTypeW;
use windows_sys::Win32::System::WindowsProgramming::DRIVE_FIXED;

/// Moves `path` to the Recycle Bin after checking, relative to its open parent, that the entry is
/// still `expect`. The move itself goes by path: an object swapped in during the microseconds
/// between the check and the move reaches the bin, where it is recoverable, and its `FileId` in
/// `Trashed::arrived` tells core it was not the approved one. Never retried, never deleted
/// (SAFE-12).
pub(crate) fn trash(
    parent: &DirHandle,
    path: &AbsPath,
    expect: FileId,
) -> Result<Trashed, DiskError> {
    let name = path.name().ok_or(DiskError::NotAFolder)?;
    if stat(parent, name)?.node.id != expect {
        return Err(DiskError::NotAFolder);
    }
    if !on_fixed_drive(path) {
        return Err(DiskError::NoTrash);
    }
    let _apartment = Apartment::enter()?;
    let at = recycle(path)?;
    let landed = AbsPath::parse(at.as_os_str()).map_err(|_| DiskError::NoTrash)?;
    match node(&landed) {
        Seen::Found(arrived) => Ok(Trashed {
            from: path.as_path().to_path_buf(),
            at,
            arrived: arrived.id,
        }),
        _ => Err(DiskError::NoTrash),
    }
}

/// A removable or network drive may have no bin, and the shell then deletes.
fn on_fixed_drive(path: &AbsPath) -> bool {
    let Some(Component::Prefix(prefix)) = path.as_path().components().next() else {
        return false;
    };
    let root: Vec<u16> = prefix
        .as_os_str()
        .encode_wide()
        .chain([u16::from(b'\\'), 0])
        .collect();
    // SAFETY: `root` is NUL-terminated and outlives the call.
    unsafe { GetDriveTypeW(root.as_ptr()) == DRIVE_FIXED }
}

/// This thread's single-threaded apartment, which `IFileOperation` needs, for one call.
struct Apartment;

impl Apartment {
    fn enter() -> Result<Self, DiskError> {
        // SAFETY: the reserved pointer is null, and `Drop` balances a call that succeeded.
        let entered =
            unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) };
        // `S_FALSE`, a thread already in this apartment, also needs its `CoUninitialize`. A thread
        // in the multithreaded apartment fails here, and `IFileOperation` does not run there.
        if entered.is_ok() {
            Ok(Self)
        } else {
            Err(DiskError::NoTrash)
        }
    }
}

impl Drop for Apartment {
    fn drop(&mut self) {
        // SAFETY: balances the `CoInitializeEx` that succeeded in `enter`. `trash` releases every
        // COM object of the call before this runs.
        unsafe { CoUninitialize() };
    }
}

/// A refusal the user can act on is `Denied`. Every other one is `NoTrash`.
fn refused(code: HRESULT) -> DiskError {
    if code == E_ACCESSDENIED || code == COPYENGINE_E_ACCESS_DENIED_SRC {
        DiskError::Denied
    } else {
        DiskError::NoTrash
    }
}

/// Recycles `path` and answers where it landed, as the one delete the watch saw.
fn recycle(path: &AbsPath) -> Result<PathBuf, DiskError> {
    let wide: Vec<u16> = path
        .as_path()
        .as_os_str()
        .encode_wide()
        .chain([0])
        .collect();
    let watch = ComObject::new(Watch::default());
    let failed = |e: windows::core::Error| refused(e.code());
    // SAFETY: COM is initialized on this thread for the whole call, `wide` is NUL-terminated and
    // outlives it, and each interface used is one an earlier call in this block returned.
    let performed = unsafe {
        let operation: IFileOperation =
            CoCreateInstance(&FileOperation, None, CLSCTX_ALL).map_err(failed)?;
        let flags = FOF_ALLOWUNDO | FOF_NO_UI | FOFX_RECYCLEONDELETE | FOFX_EARLYFAILURE;
        operation.SetOperationFlags(flags).map_err(failed)?;
        let item: IShellItem =
            SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None).map_err(failed)?;
        let sink: IFileOperationProgressSink = watch.to_interface();
        operation.Advise(&sink).map_err(failed)?;
        operation.DeleteItem(&item, None).map_err(failed)?;
        operation.PerformOperations()
    };
    let landings = std::mem::take(&mut *watch.landed.lock().map_err(|_| DiskError::NoTrash)?);
    match landings.as_slice() {
        [Ok(at)] => Ok(at.clone()),
        [Err(code)] => Err(refused(*code)),
        _ => Err(performed.err().map_or(DiskError::NoTrash, failed)),
    }
}

fn file_system_path(item: &IShellItem) -> windows::core::Result<PathBuf> {
    // SAFETY: `item` is live, and the string it returns is ours to free with `CoTaskMemFree`.
    unsafe {
        let name = item.GetDisplayName(SIGDN_FILESYSPATH)?;
        let path = PathBuf::from(OsString::from_wide(name.as_wide()));
        CoTaskMemFree(Some(name.0.cast_const().cast()));
        Ok(path)
    }
}

/// Refuses a delete the shell will not recycle, and records where each recycled item landed.
#[implement(IFileOperationProgressSink)]
#[derive(Default)]
struct Watch {
    landed: Mutex<Vec<Result<PathBuf, HRESULT>>>,
}

impl IFileOperationProgressSink_Impl for Watch_Impl {
    fn PreDeleteItem(&self, flags: u32, _: Ref<IShellItem>) -> windows::core::Result<()> {
        if flags & TSF_DELETE_RECYCLE_IF_POSSIBLE.0 as u32 == 0 {
            return Err(E_ABORT.into());
        }
        Ok(())
    }

    fn PostDeleteItem(
        &self,
        _: u32,
        _: Ref<IShellItem>,
        result: HRESULT,
        created: Ref<IShellItem>,
    ) -> windows::core::Result<()> {
        let landed = result
            .ok()
            .and_then(|()| file_system_path(created.ok()?))
            .map_err(|e| e.code());
        if let Ok(mut landings) = self.landed.lock() {
            landings.push(landed);
        }
        Ok(())
    }

    fn StartOperations(&self) -> windows::core::Result<()> {
        Ok(())
    }

    fn FinishOperations(&self, _: HRESULT) -> windows::core::Result<()> {
        Ok(())
    }

    fn PreRenameItem(&self, _: u32, _: Ref<IShellItem>, _: &PCWSTR) -> windows::core::Result<()> {
        Ok(())
    }

    fn PostRenameItem(
        &self,
        _: u32,
        _: Ref<IShellItem>,
        _: &PCWSTR,
        _: HRESULT,
        _: Ref<IShellItem>,
    ) -> windows::core::Result<()> {
        Ok(())
    }

    fn PreMoveItem(
        &self,
        _: u32,
        _: Ref<IShellItem>,
        _: Ref<IShellItem>,
        _: &PCWSTR,
    ) -> windows::core::Result<()> {
        Ok(())
    }

    fn PostMoveItem(
        &self,
        _: u32,
        _: Ref<IShellItem>,
        _: Ref<IShellItem>,
        _: &PCWSTR,
        _: HRESULT,
        _: Ref<IShellItem>,
    ) -> windows::core::Result<()> {
        Ok(())
    }

    fn PreCopyItem(
        &self,
        _: u32,
        _: Ref<IShellItem>,
        _: Ref<IShellItem>,
        _: &PCWSTR,
    ) -> windows::core::Result<()> {
        Ok(())
    }

    fn PostCopyItem(
        &self,
        _: u32,
        _: Ref<IShellItem>,
        _: Ref<IShellItem>,
        _: &PCWSTR,
        _: HRESULT,
        _: Ref<IShellItem>,
    ) -> windows::core::Result<()> {
        Ok(())
    }

    fn PreNewItem(&self, _: u32, _: Ref<IShellItem>, _: &PCWSTR) -> windows::core::Result<()> {
        Ok(())
    }

    fn PostNewItem(
        &self,
        _: u32,
        _: Ref<IShellItem>,
        _: &PCWSTR,
        _: &PCWSTR,
        _: u32,
        _: HRESULT,
        _: Ref<IShellItem>,
    ) -> windows::core::Result<()> {
        Ok(())
    }

    fn UpdateProgress(&self, _: u32, _: u32) -> windows::core::Result<()> {
        Ok(())
    }

    fn ResetTimer(&self) -> windows::core::Result<()> {
        Ok(())
    }

    fn PauseTimer(&self) -> windows::core::Result<()> {
        Ok(())
    }

    fn ResumeTimer(&self) -> windows::core::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::windows::delete::open_path;
    use crate::windows::delete::tests::{abs, exists, temp};
    use std::ffi::OsStr;

    #[test]
    fn a_file_reaches_the_recycle_bin_as_itself() {
        let (_dir, base) = temp();
        let file = base.join("recycle-me.txt");
        std::fs::write(&file, b"x").unwrap();
        let parent = open_path(&abs(&base)).unwrap();
        let expect = stat(&parent, OsStr::new("recycle-me.txt")).unwrap().node.id;

        let moved = trash(&parent, &abs(&file), expect).unwrap();
        assert!(!exists(&file));
        assert_eq!(moved.arrived, expect);
        assert!(
            moved.at.to_string_lossy().contains("$Recycle.Bin"),
            "{moved:?}"
        );
    }

    #[test]
    fn a_folder_reaches_the_recycle_bin_whole() {
        let (_dir, base) = temp();
        let folder = base.join("recycle-me");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("inside.txt"), b"x").unwrap();
        let parent = open_path(&abs(&base)).unwrap();
        let expect = stat(&parent, OsStr::new("recycle-me")).unwrap().node.id;

        let moved = trash(&parent, &abs(&folder), expect).unwrap();
        assert!(!exists(&folder));
        assert_eq!(moved.arrived, expect);
        assert!(exists(&moved.at.join("inside.txt")), "{moved:?}");
    }

    #[test]
    fn an_entry_that_changed_stays_where_it_is() {
        let (_dir, base) = temp();
        let file = base.join("keep-me.txt");
        std::fs::write(&file, b"x").unwrap();
        let parent = open_path(&abs(&base)).unwrap();
        let other = crate::windows::delete::id(&parent).unwrap();

        assert_eq!(
            trash(&parent, &abs(&file), other),
            Err(DiskError::NotAFolder)
        );
        assert!(exists(&file));
    }

    #[test]
    fn the_watch_refuses_a_delete_the_shell_would_not_recycle() {
        let sink: IFileOperationProgressSink = Watch::default().into();
        let recycle = TSF_DELETE_RECYCLE_IF_POSSIBLE.0 as u32;
        // SAFETY: `sink` is live, and a null item is one the watch never reads.
        let (for_good, recycled) = unsafe {
            (
                sink.PreDeleteItem(0, None),
                sink.PreDeleteItem(recycle, None),
            )
        };
        assert_eq!(for_good.map_err(|e| e.code()), Err(E_ABORT));
        assert_eq!(recycled.map_err(|e| e.code()), Ok(()));
    }
}
