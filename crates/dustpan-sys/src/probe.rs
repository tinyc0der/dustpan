use dustpan_core::identity::Node;
use dustpan_core::path::AbsPath;
use dustpan_core::probe::{Handles, Probe, ProcessTable, Seen, Unseen};
use std::io::{self, ErrorKind};
use std::path::PathBuf;

#[cfg(unix)]
use crate::unix::identity;
#[cfg(windows)]
use crate::windows::identity;

#[cfg(target_os = "linux")]
use crate::linux::process;
#[cfg(target_os = "macos")]
use crate::macos::process;
#[cfg(windows)]
use crate::windows::process;

/// The real [`Probe`]. Holds nothing between calls, so every answer is a fresh observation and
/// `Guard::recheck` sees the disk as it is at the sink.
#[derive(Default)]
pub struct LiveProbe {
    _private: (),
}

impl LiveProbe {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Probe for LiveProbe {
    fn node(&self, path: &AbsPath) -> Seen<Node> {
        identity::node(path)
    }

    fn link_target(&self, link: &AbsPath) -> Seen<PathBuf> {
        match std::fs::read_link(link.as_path()) {
            Ok(target) => Seen::Found(target),
            Err(e) => unseen(&e),
        }
    }

    fn spelling(&self, path: &AbsPath) -> Seen<AbsPath> {
        #[cfg(target_os = "macos")]
        return crate::macos::spelling::stored(path);
        #[cfg(windows)]
        return identity::stored(path);
        // A name is its own spelling on a case-sensitive file system, and the guard refuses a
        // linked ancestor before it asks this.
        #[cfg(not(any(target_os = "macos", windows)))]
        return Seen::Found(path.clone());
    }

    fn processes(&self) -> Seen<ProcessTable> {
        process::table()
    }

    /// M3 implements this with SAFE-08 and SAFE-09. Until then every SQLite-family and
    /// partial-download path is kept as "in use unknown", which fails closed.
    fn in_use(&self, _files: &[AbsPath]) -> Handles {
        Handles::Unknown(Unseen::Unsupported)
    }
}

pub(crate) fn unseen<T>(e: &io::Error) -> Seen<T> {
    match e.kind() {
        ErrorKind::NotFound | ErrorKind::NotADirectory => Seen::Missing,
        ErrorKind::PermissionDenied => Seen::Unknown(Unseen::Denied),
        _ => Seen::Unknown(Unseen::Failed),
    }
}
