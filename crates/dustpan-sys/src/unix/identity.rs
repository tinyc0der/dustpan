use crate::probe::unseen;
use dustpan_core::identity::{FileId, Node, NodeKind};
use dustpan_core::path::AbsPath;
use dustpan_core::probe::Seen;
use std::os::unix::fs::MetadataExt as _;

/// One `lstat`.
pub(crate) fn node(path: &AbsPath) -> Seen<Node> {
    match std::fs::symlink_metadata(path.as_path()) {
        Ok(meta) => {
            let t = meta.file_type();
            let kind = if t.is_symlink() {
                NodeKind::Link
            } else if t.is_dir() {
                NodeKind::Dir
            } else if t.is_file() {
                NodeKind::File
            } else {
                NodeKind::Other
            };
            let id = FileId::new(meta.dev(), u128::from(meta.ino()));
            Seen::Found(Node { kind, id })
        }
        Err(e) => unseen(&e),
    }
}
