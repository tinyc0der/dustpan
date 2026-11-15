use std::collections::{HashMap, HashSet};
use std::ffi::{CString, OsStr, OsString};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Default)]
struct Tally {
    per_entry: Mutex<HashMap<OsString, u64>>,
    seen_links: Mutex<HashSet<(u64, u64)>>,
    seen_clones: Mutex<HashSet<(u64, u64)>>,
    files: AtomicU64,
    errors: AtomicU64,
}

impl Tally {
    fn add(&self, top: &OsStr, bytes: u64) {
        *self.per_entry.lock().unwrap().entry(top.to_owned()).or_default() += bytes;
    }

    fn first_sighting(&self, nlink: u64, dev: u64, ino: u64) -> bool {
        nlink <= 1 || self.seen_links.lock().unwrap().insert((dev, ino))
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let method = args.get(1).map(String::as_str).unwrap_or("");
    let root = PathBuf::from(args.get(2).expect("usage: s1walk <du|jwalk|bulk|bulk-mole> <root> [threads]"));
    let threads: usize = args
        .get(3)
        .and_then(|t| t.parse().ok())
        .unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4));
    let tally: &'static Tally = Box::leak(Box::default());
    let start = Instant::now();
    let workers = match method {
        "du" => {
            du_workers(&root, 4, tally);
            4
        }
        "jwalk" => {
            jwalk_walk(&root, threads, tally);
            threads
        }
        "bulk" => {
            bulk_walk(&root, threads, tally, Semantics::Du);
            threads
        }
        "bulk-mole" => {
            bulk_walk(&root, threads, tally, Semantics::Mole);
            threads
        }
        other => panic!("unknown method {other}"),
    };
    let wall_ms = start.elapsed().as_millis();
    let per_entry = std::mem::take(&mut *tally.per_entry.lock().unwrap());
    let total: u64 = per_entry.values().sum();
    println!("method\t{method}");
    println!("root\t{}", root.display());
    println!("workers\t{workers}");
    println!("wall_ms\t{wall_ms}");
    println!("total\t{total}");
    println!("files\t{}", tally.files.load(Ordering::Relaxed));
    println!("errors\t{}", tally.errors.load(Ordering::Relaxed));
    let mut rows: Vec<_> = per_entry.into_iter().collect();
    rows.sort();
    for (name, bytes) in rows {
        println!("entry\t{}\t{bytes}", name.to_string_lossy());
    }
}

fn du_workers(root: &Path, workers: usize, tally: &Tally) {
    let root_dev = std::fs::symlink_metadata(root).unwrap().dev();
    let mut queue = Vec::new();
    for entry in std::fs::read_dir(root).unwrap().flatten() {
        let Ok(meta) = entry.metadata() else {
            tally.errors.fetch_add(1, Ordering::Relaxed);
            continue;
        };
        if meta.is_dir() {
            if meta.dev() == root_dev {
                queue.push(entry.path());
            } else {
                tally.add(&entry.file_name(), 0);
            }
        } else {
            tally.files.fetch_add(1, Ordering::Relaxed);
            let bytes = if tally.first_sighting(meta.nlink(), meta.dev(), meta.ino()) { meta.blocks() * 512 } else { 0 };
            tally.add(&entry.file_name(), bytes);
        }
    }
    let queue = Mutex::new(queue);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                let Some(path) = queue.lock().unwrap().pop() else { break };
                let out = Command::new("/usr/bin/du").arg("-skPx").arg(&path).output().unwrap();
                if !out.status.success() {
                    tally.errors.fetch_add(1, Ordering::Relaxed);
                }
                let stdout = String::from_utf8_lossy(&out.stdout);
                let kib: u64 = stdout
                    .lines()
                    .last()
                    .and_then(|l| l.split('\t').next())
                    .and_then(|k| k.trim().parse().ok())
                    .unwrap_or(0);
                tally.add(path.file_name().unwrap(), kib * 1024);
            });
        }
    });
}

fn jwalk_walk(root: &Path, threads: usize, tally: &'static Tally) {
    let root_dev = std::fs::symlink_metadata(root).unwrap().dev();
    let root_buf = root.to_path_buf();
    let walk = jwalk::WalkDirGeneric::<((), ())>::new(root)
        .skip_hidden(false)
        .follow_links(false)
        .sort(false)
        .parallelism(jwalk::Parallelism::RayonNewPool(threads))
        .process_read_dir(move |depth, dir, _state, children| {
            if depth.is_none() {
                return;
            }
            let top_of_dir = if depth == Some(0) {
                None
            } else {
                dir.strip_prefix(&root_buf).ok().and_then(|rel| rel.components().next()).map(|c| c.as_os_str().to_owned())
            };
            let mut subtotal = 0u64;
            for child in children.iter_mut().flatten() {
                let meta = match std::fs::symlink_metadata(child.path()) {
                    Ok(m) => m,
                    Err(_) => {
                        tally.errors.fetch_add(1, Ordering::Relaxed);
                        child.read_children_path = None;
                        continue;
                    }
                };
                let other_device = meta.is_dir() && meta.dev() != root_dev;
                if other_device {
                    child.read_children_path = None;
                }
                let bytes = if other_device || !tally.first_sighting(meta.nlink(), meta.dev(), meta.ino()) {
                    0
                } else {
                    meta.blocks() * 512
                };
                if !meta.is_dir() {
                    tally.files.fetch_add(1, Ordering::Relaxed);
                }
                match &top_of_dir {
                    None => tally.add(&child.file_name, bytes),
                    Some(_) => subtotal += bytes,
                }
            }
            if let Some(top) = top_of_dir {
                tally.add(&top, subtotal);
            }
        });
    for entry in walk {
        if entry.is_err() {
            tally.errors.fetch_add(1, Ordering::Relaxed);
        }
    }
}

const VREG: u32 = 1;
const VDIR: u32 = 2;

const ATTR_CMNEXT_EXT_FLAGS: u32 = 0x0000_0200;
const EF_SHARES_ALL_BLOCKS: u64 = 0x40;

#[derive(Clone, Copy, PartialEq)]
enum Semantics {
    Du,
    Mole,
}

struct BulkEntry {
    name: OsString,
    kind: u32,
    fileid: u64,
    nlink: u32,
    alloc: u64,
    data_len: u64,
    clone_id: u64,
    ext_flags: u64,
}

impl BulkEntry {
    fn counted_bytes(&self, sem: Semantics, dev: u64, tally: &Tally) -> u64 {
        if self.kind == VREG && !tally.first_sighting(self.nlink as u64, dev, self.fileid) {
            return 0;
        }
        if sem == Semantics::Du {
            return self.alloc;
        }
        let size = self.alloc.min(self.data_len);
        let shared_clone = self.kind == VREG && size > 0 && self.ext_flags & EF_SHARES_ALL_BLOCKS != 0;
        if shared_clone && !tally.seen_clones.lock().unwrap().insert((dev, self.clone_id)) {
            return 0;
        }
        size
    }
}

fn bulk_list(fd: libc::c_int, buf: &mut [u8], sem: Semantics) -> Result<Vec<BulkEntry>, ()> {
    let mole = sem == Semantics::Mole;
    let mut attrs = libc::attrlist {
        bitmapcount: libc::ATTR_BIT_MAP_COUNT,
        reserved: 0,
        commonattr: libc::ATTR_CMN_RETURNED_ATTRS | libc::ATTR_CMN_NAME | libc::ATTR_CMN_OBJTYPE | libc::ATTR_CMN_FILEID,
        volattr: 0,
        dirattr: 0,
        fileattr: libc::ATTR_FILE_LINKCOUNT | libc::ATTR_FILE_ALLOCSIZE | if mole { libc::ATTR_FILE_DATALENGTH } else { 0 },
        forkattr: if mole { libc::ATTR_CMNEXT_CLONEID | ATTR_CMNEXT_EXT_FLAGS } else { 0 },
    };
    let options = libc::FSOPT_PACK_INVAL_ATTRS | if mole { libc::FSOPT_ATTR_CMN_EXTENDED } else { 0 };
    let mut out = Vec::new();
    loop {
        let n = unsafe {
            libc::getattrlistbulk(
                fd,
                &mut attrs as *mut _ as *mut libc::c_void,
                buf.as_mut_ptr() as *mut libc::c_void,
                buf.len(),
                options as u64,
            )
        };
        if n < 0 {
            return Err(());
        }
        if n == 0 {
            return Ok(out);
        }
        let mut p = 0usize;
        for _ in 0..n {
            let rd_u32 = |at: usize| u32::from_ne_bytes(buf[at..at + 4].try_into().unwrap());
            let rd_u64 = |at: usize| u64::from_ne_bytes(buf[at..at + 8].try_into().unwrap());
            let len = rd_u32(p) as usize;
            let returned_file = rd_u32(p + 4 + 12);
            let mut q = p + 4 + 20;
            let name_ref = q;
            let name_off = i32::from_ne_bytes(buf[q..q + 4].try_into().unwrap());
            let name_len = rd_u32(q + 4) as usize;
            q += 8;
            let kind = rd_u32(q);
            q += 4;
            let fileid = rd_u64(q);
            q += 8;
            let nlink = rd_u32(q);
            q += 4;
            let alloc = rd_u64(q);
            q += 8;
            let end = p + len;
            let field = |at: usize| if mole && at + 8 <= end { rd_u64(at) } else { 0 };
            let (data_len, clone_id, ext_flags) = (field(q), field(q + 8), field(q + 16));
            let name_start = (name_ref as isize + name_off as isize) as usize;
            let name = OsStr::from_bytes(&buf[name_start..name_start + name_len.saturating_sub(1)]).to_owned();
            let alloc = if returned_file & libc::ATTR_FILE_ALLOCSIZE != 0 { alloc } else { 0 };
            out.push(BulkEntry { name, kind, fileid, nlink, alloc, data_len, clone_id, ext_flags });
            p += len;
        }
    }
}

fn open_dir(path: &Path) -> Option<(libc::c_int, u64)> {
    let c = CString::new(path.as_os_str().as_bytes()).ok()?;
    let fd = unsafe { libc::open(c.as_ptr(), libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC) };
    if fd < 0 {
        return None;
    }
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstat(fd, &mut st) } != 0 {
        unsafe { libc::close(fd) };
        return None;
    }
    Some((fd, st.st_dev as u64))
}

fn bulk_walk(root: &Path, threads: usize, tally: &Tally, sem: Semantics) {
    let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().unwrap();
    let (root_fd, root_dev) = open_dir(root).expect("open root");
    let mut buf = vec![0u8; 256 * 1024];
    let top = bulk_list(root_fd, &mut buf, sem).expect("list root");
    unsafe { libc::close(root_fd) };
    let counters: Vec<AtomicU64> = top.iter().map(|_| AtomicU64::new(0)).collect();
    pool.scope(|scope| {
        for (idx, entry) in top.iter().enumerate() {
            if entry.kind == VDIR {
                let path = root.join(&entry.name);
                let counters = &counters;
                scope.spawn(move |s| bulk_dir(s, path, root_dev, idx, counters, tally, sem));
            } else {
                tally.files.fetch_add(1, Ordering::Relaxed);
                counters[idx].fetch_add(entry.counted_bytes(sem, root_dev, tally), Ordering::Relaxed);
            }
        }
    });
    for (entry, counter) in top.iter().zip(counters) {
        tally.add(&entry.name, counter.into_inner());
    }
}

fn bulk_dir<'s>(
    scope: &rayon::Scope<'s>,
    path: PathBuf,
    root_dev: u64,
    idx: usize,
    counters: &'s [AtomicU64],
    tally: &'s Tally,
    sem: Semantics,
) {
    let Some((fd, dev)) = open_dir(&path) else {
        tally.errors.fetch_add(1, Ordering::Relaxed);
        return;
    };
    if dev != root_dev {
        unsafe { libc::close(fd) };
        return;
    }
    thread_local! { static BUF: std::cell::RefCell<Vec<u8>> = std::cell::RefCell::new(vec![0u8; 256 * 1024]); }
    let listed = BUF.with(|b| bulk_list(fd, &mut b.borrow_mut(), sem));
    unsafe { libc::close(fd) };
    let Ok(entries) = listed else {
        tally.errors.fetch_add(1, Ordering::Relaxed);
        return;
    };
    let mut subtotal = 0u64;
    let path = Arc::new(path);
    for entry in entries {
        if entry.kind == VDIR {
            let child = path.join(&entry.name);
            scope.spawn(move |s| bulk_dir(s, child, root_dev, idx, counters, tally, sem));
        } else {
            tally.files.fetch_add(1, Ordering::Relaxed);
            subtotal += entry.counted_bytes(sem, root_dev, tally);
        }
    }
    counters[idx].fetch_add(subtotal, Ordering::Relaxed);
}
