//! A folder is kept while a process holds something in it that the snapshot of open files
//! cannot place: a file whose path is too long for the kernel to report whole, or a socket
//! bound through a spelling that does not resolve (SAFE-09). `recheck` refuses it too.
#![cfg(target_os = "macos")]

use dustpan_core::candidate::{Candidate, GroupId, Rule, RuleId, Sink};
use dustpan_core::guard::{Guard, GuardInputs, Homes, KeptReason, Rejection, Verdict};
use dustpan_core::path::AbsPath;
use dustpan_core::paths::Os;
use dustpan_core::probe::{OpenFiles, Pid, Probe, Seen};
use dustpan_core::whitelist::Whitelist;
use dustpan_sys::LiveProbe;
use std::ffi::CString;
use std::io::{BufRead as _, BufReader};
use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::fs::MetadataExt as _;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

const FOLDER: &str = "DUSTPAN_TEST_HOLD_IN";
const CHAIN: &str = "DUSTPAN_TEST_HOLD_CHAIN";
const HELPER: &str = "helper_holds_a_file_or_a_socket_when_asked";

/// One holder that cannot be placed keeps every path no rule protects, for as long as it
/// lives. The tests below would see each other's holder.
static ONE_HOLDER: Mutex<()> = Mutex::new(());

fn one_holder() -> MutexGuard<'static, ()> {
    ONE_HOLDER.lock().unwrap_or_else(PoisonError::into_inner)
}

fn c_string(name: &Path) -> CString {
    CString::new(name.as_os_str().as_bytes()).unwrap()
}

/// What this binary runs as a child of the tests below. Given a chain of names, it makes each
/// folder of the chain below the folder it is given and holds the file at the end open. It
/// walks the chain one descriptor at a time, because the whole path can be longer than a
/// system call takes. Given no chain, it binds a socket in the folder through the folder's
/// volfs spelling. Then it says `held` and waits to be killed.
#[test]
fn helper_holds_a_file_or_a_socket_when_asked() {
    let Some(folder) = std::env::var_os(FOLDER) else {
        return;
    };
    let folder = PathBuf::from(folder);
    let _held: Box<dyn std::any::Any> = match std::env::var_os(CHAIN) {
        Some(chain) => {
            let chain = PathBuf::from(chain);
            let mut names: Vec<_> = chain.iter().map(|name| c_string(name.as_ref())).collect();
            let file = names.pop().unwrap();
            // SAFETY: every path handed to the calls below is a NUL-terminated string, and
            // every descriptor is one an earlier call returned and nothing closed.
            unsafe {
                let mut at = libc::open(c_string(&folder).as_ptr(), libc::O_RDONLY);
                assert!(at >= 0);
                for name in &names {
                    libc::mkdirat(at, name.as_ptr(), 0o700);
                    let below = libc::openat(at, name.as_ptr(), libc::O_RDONLY);
                    assert!(below >= 0);
                    libc::close(at);
                    at = below;
                }
                let held = libc::openat(at, file.as_ptr(), libc::O_RDWR | libc::O_CREAT, 0o600);
                assert!(held >= 0);
                libc::close(at);
            }
            Box::new(())
        }
        None => {
            let at = std::fs::metadata(&folder).unwrap();
            let address = format!("/.vol/{}/{}/held.sock", at.dev(), at.ino());
            Box::new(UnixListener::bind(address).unwrap())
        }
    };
    println!("held");
    std::thread::sleep(std::time::Duration::from_secs(120));
}

struct Holder(Child);

impl Holder {
    fn start(folder: &Path, chain: Option<&Path>) -> Self {
        let mut holder = Command::new(std::env::current_exe().unwrap());
        holder
            .args(["--exact", HELPER, "--nocapture"])
            .env(FOLDER, folder)
            .stdout(Stdio::piped());
        if let Some(chain) = chain {
            holder.env(CHAIN, chain);
        }
        let mut holder = holder.spawn().unwrap();
        let mut said = BufReader::new(holder.stdout.take().unwrap()).lines();
        assert!(
            said.any(|line| line.unwrap() == "held"),
            "the helper never said held"
        );
        Self(holder)
    }

    fn pid(&self) -> Pid {
        Pid(self.0.id())
    }
}

impl Drop for Holder {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// A home under a temporary folder, with the cache folders the test names.
struct Home {
    _dir: tempfile::TempDir,
    caches: PathBuf,
    guard: Guard,
    probe: LiveProbe,
}

impl Home {
    fn with(folders: &[&str]) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().canonicalize().unwrap();
        let caches = home.join("Library/Caches");
        for folder in folders {
            std::fs::create_dir_all(caches.join(folder)).unwrap();
        }
        let probe = LiveProbe::new();
        let env = |_: &str| None;
        let home = AbsPath::parse(home.as_ref()).unwrap();
        let inputs = GuardInputs {
            os: Os::MacOs,
            whitelist: Whitelist::defaults(Os::MacOs, &home),
            homes: Homes::new(home, None),
            env: &env,
        };
        Self {
            _dir: dir,
            caches,
            guard: Guard::new(inputs, &probe).unwrap(),
            probe,
        }
    }

    /// Judges a cache folder the way a scan does.
    fn discover(&self, folder: &str) -> Candidate {
        let path = AbsPath::parse(self.caches.join(folder).as_ref()).unwrap();
        let Seen::Found(node) = self.probe.node(&path) else {
            panic!("{folder} is not there");
        };
        let rule = Arc::new(Rule {
            id: RuleId("test.cache".into()),
            group: GroupId("test".into()),
            owners: Box::new([]),
            sink: Sink::Delete,
            default_selected: true,
        });
        let mut pass = self.guard.pass(&self.probe);
        pass.candidate(path, node, &rule).unwrap()
    }

    /// A candidate found while nothing held the folder, as a scan finds it before the sink.
    fn discover_idle(&self, folder: &str) -> Candidate {
        let candidate = self.discover(folder);
        assert_eq!(candidate.verdict(), &Verdict::Eligible, "{folder}");
        candidate
    }

    fn assert_kept_as_live(&self, folder: &str, found_idle: Candidate) {
        let verdict = self.discover(folder).verdict().clone();
        assert!(
            matches!(verdict, Verdict::Kept(KeptReason::Live(_))),
            "{folder} while a process holds a file in it: {verdict}"
        );
        match self.guard.recheck(found_idle, &self.probe) {
            Ok(approved) => panic!("{folder} was approved for {:?}", approved.sink()),
            Err(refused) => assert!(
                matches!(refused.rejection, Rejection::Kept(KeptReason::Live(_))),
                "{:?}",
                refused.rejection
            ),
        }
    }

    fn open_files(&self) -> OpenFiles {
        match self.probe.open_files() {
            Seen::Found(open) => open,
            other => panic!("no snapshot of open files: {other:?}"),
        }
    }
}

/// A relative path of `len` bytes: folders named with up to 200 bytes each, then `file`.
fn chain(letter: char, len: usize, file: &str) -> PathBuf {
    let mut chain = PathBuf::new();
    let mut left = len - file.len();
    while left > 0 {
        // A name and its separator. No name is left with only the separator to take.
        let name = if left == 202 { 199 } else { left.min(201) - 1 };
        chain.push(letter.to_string().repeat(name));
        left -= name + 1;
    }
    let chain = chain.join(file);
    assert_eq!(chain.as_os_str().len(), len);
    chain
}

/// Folder `below` at `path`, where `path` is `len` bytes long in all.
fn at_length(below: &Path, letter: char, len: usize, file: &str) -> PathBuf {
    chain(letter, len - below.as_os_str().len() - 1, file)
}

#[test]
fn a_folder_with_a_file_held_past_the_longest_path_is_kept() {
    let _one = one_holder();
    let home = Home::with(&["vendor"]);
    let found_idle = home.discover_idle("vendor");
    let _holder = Holder::start(
        &home.caches.join("vendor"),
        Some(&chain('d', 1300, "held.db")),
    );
    home.assert_kept_as_live("vendor", found_idle);
}

/// The kernel reports the end of a path that does not fit. Here that end spells a file in a
/// folder nothing holds.
#[test]
fn the_end_of_a_long_path_does_not_name_a_folder_nothing_holds() {
    let _one = one_holder();
    let home = Home::with(&["vendor-held", "idle"]);
    let idle = home.caches.join("idle");
    let found_idle = home.discover_idle("vendor-held");
    let spelled = idle.join(at_length(&idle, 'a', 1000, "x"));
    let below_held = Path::new(&"p".repeat(200)).join(spelled.strip_prefix("/").unwrap());
    let holder = Holder::start(&home.caches.join("vendor-held"), Some(&below_held));

    let open = home.open_files();
    let in_idle: Vec<_> = open
        .files
        .iter()
        .filter(|file| file.pid == holder.pid() && file.path.starts_with(&idle))
        .collect();
    assert_eq!(in_idle, [] as [&_; 0]);
    home.assert_kept_as_live("vendor-held", found_idle);
}

#[test]
fn a_folder_with_a_socket_bound_through_volfs_is_kept() {
    let _one = one_holder();
    let home = Home::with(&["vendor"]);
    let found_idle = home.discover_idle("vendor");
    let _holder = Holder::start(&home.caches.join("vendor"), None);
    home.assert_kept_as_live("vendor", found_idle);
}

/// A long path the kernel does report whole names its holder, as a short one does.
#[test]
fn a_file_held_at_a_long_path_that_fits_is_listed_by_its_path() {
    let _one = one_holder();
    let home = Home::with(&["vendor"]);
    let vendor = home.caches.join("vendor");
    let chain = at_length(&vendor, 'd', 900, "held.db");
    let held = vendor.join(&chain);
    let holder = Holder::start(&vendor, Some(&chain));

    let open = home.open_files();
    let holders: Vec<_> = open.files.iter().filter(|file| file.path == held).collect();
    assert_eq!(holders.len(), 1, "{holders:?}");
    assert_eq!(holders[0].pid, holder.pid());
    assert!(
        open.unlisted
            .iter()
            .all(|process| process.pid != holder.pid())
    );
}
