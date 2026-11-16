use dustpan_core::identity::Node;
use dustpan_core::path::AbsPath;
use dustpan_core::probe::{OpenFiles, Probe, ProcessTable, Seen, Unseen};
use std::io::{self, ErrorKind};
use std::path::PathBuf;

#[cfg(unix)]
use crate::unix::identity;
#[cfg(windows)]
use crate::windows::identity;

#[cfg(target_os = "linux")]
use crate::linux::{open, process};
#[cfg(target_os = "macos")]
use crate::macos::{open, process};
#[cfg(windows)]
use crate::windows::process::{self, open};

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

    fn open_files(&self) -> Seen<OpenFiles> {
        open::open_files()
    }
}

/// The most descriptors one snapshot of open files looks at: about 0.4 s of kernel calls on the
/// bench host, at 1.6 µs each. No kernel limit holds the total down, because `dup` and `fork`
/// add descriptors that share one open file. Past it the snapshot answers "timed out", which
/// keeps every path no rule protects.
#[cfg(unix)]
pub(crate) const MAX_DESCRIPTORS: usize = 1 << 18;

/// The address a Unix socket is bound to, with its folder resolved to a physical path. The
/// kernel keeps the address as the process spelled it, and `$TMPDIR/x.sock` is below the link
/// `/var` on macOS. A folder that does not resolve leaves the address as spelled. `None` for a
/// relative address: only the folder the process worked in at the time could resolve it.
#[cfg(unix)]
pub(crate) fn bound_socket(address: PathBuf) -> Option<PathBuf> {
    if !address.is_absolute() {
        return None;
    }
    let folder = address.parent().and_then(|dir| dir.canonicalize().ok());
    Some(match (folder, address.file_name()) {
        (Some(folder), Some(name)) => folder.join(name),
        _ => address,
    })
}

pub(crate) fn unseen<T>(e: &io::Error) -> Seen<T> {
    match e.kind() {
        ErrorKind::NotFound | ErrorKind::NotADirectory => Seen::Missing,
        ErrorKind::PermissionDenied => Seen::Unknown(Unseen::Denied),
        _ => Seen::Unknown(Unseen::Failed),
    }
}
