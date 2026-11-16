//! Windows parts of the Delete sink. Not built yet: every call fails, so no Windows sink
//! removes anything.
use crate::disk::DirHandle;
use dustpan_core::disk::{DiskError, Unlink};
use dustpan_core::identity::{DirEntry, FileId};
use dustpan_core::path::AbsPath;
use std::ffi::OsStr;

/// `ERROR_CALL_NOT_IMPLEMENTED`.
const NOT_BUILT: DiskError = DiskError::Other(120);

pub(crate) fn open_path(_: &AbsPath) -> Result<DirHandle, DiskError> {
    Err(NOT_BUILT)
}

pub(crate) fn open_dir(_: &DirHandle, _: &OsStr) -> Result<DirHandle, DiskError> {
    Err(NOT_BUILT)
}

pub(crate) fn id(_: &DirHandle) -> Result<FileId, DiskError> {
    Err(NOT_BUILT)
}

pub(crate) fn stat(_: &DirHandle, _: &OsStr) -> Result<DirEntry, DiskError> {
    Err(NOT_BUILT)
}

pub(crate) fn list(
    _: &DirHandle,
    _: &mut Vec<DirEntry>,
    _: fn(&DirHandle, &OsStr) -> Result<DirEntry, DiskError>,
) -> Result<(), DiskError> {
    Err(NOT_BUILT)
}

pub(crate) fn unlink(_: &DirHandle, _: &OsStr, _: Unlink) -> Result<(), DiskError> {
    Err(NOT_BUILT)
}
