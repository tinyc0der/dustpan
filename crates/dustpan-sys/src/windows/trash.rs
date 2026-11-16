//! The Windows Trash sink (the Recycle Bin). Not built yet: every move fails as `NoTrash`, and
//! the item stays (SAFE-12).
use crate::disk::DirHandle;
use dustpan_core::disk::{DiskError, Trashed};
use dustpan_core::identity::FileId;
use dustpan_core::path::AbsPath;

pub(crate) fn trash(_: &DirHandle, _: &AbsPath, _: FileId) -> Result<Trashed, DiskError> {
    Err(DiskError::NoTrash)
}
