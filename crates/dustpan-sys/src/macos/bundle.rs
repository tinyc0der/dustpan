//! Which bundles an executable sits in, read from each bundle's `Info.plist`.
use super::plist::{Dict, Value};
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::{ErrorKind, Read as _};
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

const ID_KEY: &str = "CFBundleIdentifier";

/// The most folders above one executable that are asked for an `Info.plist`. A helper inside
/// a framework inside an app has 6, the deepest of 787 processes on the bench host. An
/// executable below more is not identified.
const MAX_LEVELS: usize = 16;

/// What one process snapshot may spend on `Info.plist` files. Any user can run a process below
/// folders full of large property lists, so both are capped, and a process whose bundles do
/// not fit is not identified. The bench host's 787 processes need 242 files, 0.77 MB in all.
struct Budget {
    bytes: u64,
    cpu: Duration,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            bytes: 8 << 20,
            cpu: Duration::from_millis(250),
        }
    }
}

/// The CPU time this thread has used. Waiting for the disk does not count, so a cold cache
/// does not spend the budget.
fn thread_cpu() -> Duration {
    let mut now = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `now` is writable, and the clock id is a constant every macOS has.
    unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &raw mut now) };
    Duration::new(now.tv_sec as u64, now.tv_nsec as u32)
}

/// What one `Info.plist` says. Kept per folder for one process snapshot: every helper of an
/// app asks about the same bundles.
#[derive(Clone)]
enum Plist {
    Id(Box<str>),
    /// No `Info.plist` here, or one that names no id: not a bundle that can own anything.
    NoId,
    Unreadable,
}

/// The `Info.plist` answers of one process snapshot, by the folder that holds the file.
#[derive(Default)]
pub(crate) struct Bundles {
    seen: HashMap<PathBuf, Plist>,
    left: Budget,
}

/// The bundle could not be read, so the process in it could be any app.
pub(crate) struct Unreadable;

impl Bundles {
    /// The id of every bundle that encloses `exe`, innermost first. A bundle keeps its
    /// `Info.plist` in `Contents` (macOS) or at its top (an iOS app on a Mac), so both places
    /// are asked on the way up.
    pub(crate) fn enclosing(&mut self, exe: &Path) -> Result<Box<[Box<str>]>, Unreadable> {
        let may_hold_plist = |dir: &&Path| {
            dir.file_name().is_some_and(|name| name == "Contents") || dir.extension().is_some()
        };
        let dirs = exe.ancestors().skip(1).filter(may_hold_plist);
        if dirs.clone().count() > MAX_LEVELS {
            return Err(Unreadable);
        }
        let mut ids = Vec::new();
        for dir in dirs {
            let plist = match self.seen.get(dir) {
                Some(plist) => plist.clone(),
                None => {
                    let plist = read(&dir.join("Info.plist"), &mut self.left);
                    self.seen.insert(dir.to_path_buf(), plist.clone());
                    plist
                }
            };
            match plist {
                Plist::Id(id) => ids.push(id),
                Plist::NoId => {}
                Plist::Unreadable => return Err(Unreadable),
            }
        }
        Ok(ids.into())
    }
}

/// The most of an `Info.plist` that is read. The largest of 5,930 on the bench host is 357 KB.
const MAX_PLIST: u64 = 1 << 20;

/// Any user can put any file next to a process they run, so the read is bounded: a link is not
/// followed, a FIFO does not block the open, and only a regular file within the cap and within
/// what the snapshot has `left` is read. Anything else is unreadable, which leaves the process
/// unidentified.
fn read(file: &Path, left: &mut Budget) -> Plist {
    let opened = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(file);
    let file = match opened {
        Ok(file) => file,
        Err(e) if matches!(e.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) => {
            return Plist::NoId;
        }
        Err(_) => return Plist::Unreadable,
    };
    let len = match file.metadata() {
        Ok(meta) if meta.is_file() && meta.len() <= MAX_PLIST => meta.len(),
        Ok(_) | Err(_) => return Plist::Unreadable,
    };
    if len > left.bytes || left.cpu.is_zero() {
        return Plist::Unreadable;
    }
    left.bytes -= len;
    let start = thread_cpu();
    // The file can grow after the size was read.
    let mut bytes = Vec::new();
    let plist = match file.take(MAX_PLIST + 1).read_to_end(&mut bytes) {
        Ok(len) if len as u64 <= MAX_PLIST => bundle_id(&bytes),
        Ok(_) | Err(_) => Plist::Unreadable,
    };
    left.cpu = left.cpu.saturating_sub(thread_cpu().saturating_sub(start));
    plist
}

/// `CFBundleIdentifier` from the bytes of an XML or binary property list.
fn bundle_id(bytes: &[u8]) -> Plist {
    match Dict::parse(bytes).and_then(|dict| dict.get(ID_KEY)) {
        Ok(Value::Text(id)) if !id.is_empty() => Plist::Id(id),
        Ok(_) => Plist::NoId,
        Err(_) => Plist::Unreadable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>CFBundleName</key><string>Example</string>
<key>CFBundleIdentifier</key><string>com.example.app</string>
</dict></plist>"#;

    fn id(plist: &Plist) -> Option<&str> {
        match plist {
            Plist::Id(id) => Some(id),
            Plist::NoId | Plist::Unreadable => None,
        }
    }

    #[test]
    fn the_bundle_id_is_read_from_a_property_list() {
        assert_eq!(id(&bundle_id(XML.as_bytes())), Some("com.example.app"));
        let no_id = XML.replace("CFBundleIdentifier", "CFBundleVersion");
        assert!(matches!(bundle_id(no_id.as_bytes()), Plist::NoId));
        assert!(matches!(
            bundle_id(b"not a property list"),
            Plist::Unreadable
        ));
        assert!(matches!(
            bundle_id(b"<plist><array/></plist>"),
            Plist::Unreadable
        ));
    }

    #[test]
    fn only_a_small_regular_file_is_read() {
        let dir = tempfile::tempdir().unwrap();
        let at = |name: &str| dir.path().join(name);

        let read = |file: &Path| read(file, &mut Budget::default());
        std::fs::write(at("plain"), XML).unwrap();
        assert_eq!(id(&read(&at("plain"))), Some("com.example.app"));
        assert!(matches!(read(&at("absent")), Plist::NoId));
        assert!(matches!(read(&at("plain/below")), Plist::NoId));

        let padded = |len: u64| format!("{XML}{}", " ".repeat(len as usize - XML.len()));
        std::fs::write(at("at-the-cap"), padded(MAX_PLIST)).unwrap();
        assert_eq!(id(&read(&at("at-the-cap"))), Some("com.example.app"));
        std::fs::write(at("over-the-cap"), padded(MAX_PLIST + 1)).unwrap();
        assert!(matches!(read(&at("over-the-cap")), Plist::Unreadable));

        std::os::unix::fs::symlink(at("plain"), at("link")).unwrap();
        assert!(matches!(read(&at("link")), Plist::Unreadable));
        std::os::unix::fs::symlink("/dev/zero", at("endless")).unwrap();
        assert!(matches!(read(&at("endless")), Plist::Unreadable));
        assert!(matches!(read(Path::new("/dev/zero")), Plist::Unreadable));

        let fifo = std::ffi::CString::new(at("fifo").as_os_str().as_encoded_bytes()).unwrap();
        // SAFETY: `fifo` is a NUL-terminated path.
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        assert!(matches!(read(&at("fifo")), Plist::Unreadable));
    }

    #[test]
    fn a_snapshot_reads_no_more_than_its_budget() {
        let dir = tempfile::tempdir().unwrap();
        let plist = dir.path().join("Info.plist");
        std::fs::write(&plist, XML).unwrap();
        let len = XML.len() as u64;
        let cpu = Duration::from_secs(1);

        let mut left = Budget {
            bytes: 2 * len,
            cpu,
        };
        assert_eq!(id(&read(&plist, &mut left)), Some("com.example.app"));
        assert_eq!(id(&read(&plist, &mut left)), Some("com.example.app"));
        assert!(matches!(read(&plist, &mut left), Plist::Unreadable));
        // A file that is not there costs nothing and is no bundle, whatever is left.
        assert!(matches!(
            read(&dir.path().join("absent"), &mut left),
            Plist::NoId
        ));

        let mut spent = Budget {
            bytes: 2 * len,
            cpu: Duration::ZERO,
        };
        assert!(matches!(read(&plist, &mut spent), Plist::Unreadable));
    }

    #[test]
    fn an_executable_below_too_many_bundle_folders_is_not_identified() {
        let dir = tempfile::tempdir().unwrap();
        let below = |levels: usize| {
            let mut exe = dir.path().canonicalize().unwrap();
            let named_like_bundles = exe.ancestors().filter(|d| d.extension().is_some()).count();
            exe.extend(std::iter::repeat_n("a.b", levels - named_like_bundles));
            exe.join("tool")
        };
        // Nothing is on disk: the count alone decides, before any file is asked for.
        let mut bundles = Bundles::default();
        assert!(bundles.enclosing(&below(MAX_LEVELS)).is_ok());
        assert!(bundles.enclosing(&below(MAX_LEVELS + 1)).is_err());
        assert_eq!(bundles.seen.len(), MAX_LEVELS);
    }
}
