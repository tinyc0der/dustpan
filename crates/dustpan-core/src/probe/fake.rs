//! An in-memory file system and process table for core tests. Compiled only for core's own unit
//! tests and with the `testkit` feature, which no shipped crate enables; a source test fails if
//! `FakeProbe` or `testkit` appears under `crates/dustpan/src` or `crates/dustpan-sys/src`
//! (SAFE-17).
use super::{Handles, Pid, Probe, Proc, ProcessTable, Seen, Unseen};
use crate::identity::{FileId, Node, NodeKind};
use crate::path::AbsPath;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

const VOLUME: u64 = 1;
const MAX_LINK_HOPS: usize = 40;

/// Builder-style fake. Paths are in host syntax. Two spellings that share a `FileId` are
/// aliases, which is how a test models case-insensitive APFS (`/SYSTEM`) or a firmlink. Like a
/// kernel, it follows links and aliases in every component but the last.
pub struct FakeProbe {
    tree: Mutex<Tree>,
    processes: Seen<ProcessTable>,
    open: BTreeSet<PathBuf>,
    in_use_unknown: bool,
    swap_on_process_probe: Mutex<Option<PathBuf>>,
    node_calls: AtomicUsize,
    process_calls: AtomicUsize,
}

#[derive(Default)]
struct Tree {
    nodes: BTreeMap<PathBuf, Entry>,
    aliases: BTreeMap<PathBuf, PathBuf>,
    next_id: u128,
}

enum Entry {
    Node(Node),
    Link(Node, PathBuf),
    Unreadable,
}

enum Lost {
    Missing,
    Unknown(Unseen),
}

fn abs(path: &str) -> AbsPath {
    AbsPath::parse(path.as_ref()).unwrap_or_else(|e| panic!("fixture path {path:?}: {e:?}"))
}

impl Tree {
    fn fresh(&mut self, kind: NodeKind, volume: u64) -> Node {
        self.next_id += 1;
        Node {
            kind,
            id: FileId::new(volume, self.next_id),
        }
    }

    fn ensure_dirs(&mut self, path: &AbsPath) {
        let mut lineage: Vec<&Path> = path.as_path().ancestors().collect();
        lineage.reverse();
        for dir in lineage {
            if !self.nodes.contains_key(dir) {
                let node = self.fresh(NodeKind::Dir, VOLUME);
                self.nodes.insert(dir.to_path_buf(), Entry::Node(node));
            }
        }
    }

    fn put(&mut self, path: &str, entry: impl FnOnce(&mut Self) -> Entry) {
        let path = abs(path);
        if let Some(parent) = path.parent() {
            self.ensure_dirs(&parent);
        }
        let entry = entry(self);
        self.nodes.insert(path.as_path().to_path_buf(), entry);
    }

    /// The key that holds `path`'s entry. Links and aliases are followed in every component but
    /// the last, and in the last too when `follow_last` is set.
    fn locate(&self, path: &Path, follow_last: bool, hops: usize) -> Result<PathBuf, Lost> {
        if hops > MAX_LINK_HOPS {
            return Err(Lost::Unknown(Unseen::Failed));
        }
        let mut cur = PathBuf::new();
        let mut names = Vec::new();
        for component in path.components() {
            match component {
                Component::Normal(name) => names.push(name),
                Component::ParentDir => drop(names.pop()),
                Component::CurDir => {}
                root => cur.push(root.as_os_str()),
            }
        }
        let count = names.len();
        for (i, name) in names.into_iter().enumerate() {
            let last = i + 1 == count;
            cur.push(name);
            if let Some(real) = self.aliases.get(&cur) {
                cur.clone_from(real);
            }
            if last && !follow_last {
                break;
            }
            match self.nodes.get(&cur) {
                Some(Entry::Link(_, target)) => {
                    let parent = cur.parent().unwrap_or(Path::new(""));
                    cur = self.locate(&parent.join(target), true, hops + 1)?;
                }
                Some(Entry::Node(node)) if node.kind == NodeKind::Dir || last => {}
                Some(Entry::Unreadable) => return Err(Lost::Unknown(Unseen::Denied)),
                // An alias below a folder that is not there, as volfs has: the walk goes on.
                None if self.aliases.keys().any(|alias| alias.starts_with(&cur)) => {}
                Some(Entry::Node(_)) | None => return Err(Lost::Missing),
            }
        }
        Ok(cur)
    }
}

impl FakeProbe {
    /// Holds the root folder on Unix, and an empty, complete process table.
    pub fn new() -> Self {
        let mut tree = Tree::default();
        if cfg!(unix) {
            tree.ensure_dirs(&abs("/"));
        }
        Self {
            tree: Mutex::new(tree),
            processes: Seen::Found(ProcessTable {
                complete: true,
                procs: Vec::new(),
            }),
            open: BTreeSet::new(),
            in_use_unknown: false,
            swap_on_process_probe: Mutex::new(None),
            node_calls: AtomicUsize::new(0),
            process_calls: AtomicUsize::new(0),
        }
    }

    fn tree(&self) -> std::sync::MutexGuard<'_, Tree> {
        self.tree.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// A directory, with any missing ancestors, each with a fresh `FileId`.
    pub fn dir(self, path: &str) -> Self {
        self.tree().ensure_dirs(&abs(path));
        self
    }

    pub fn file(self, path: &str) -> Self {
        self.tree()
            .put(path, |t| Entry::Node(t.fresh(NodeKind::File, VOLUME)));
        self
    }

    pub fn symlink(self, path: &str, target: &str) -> Self {
        self.tree().put(path, |t| {
            Entry::Link(t.fresh(NodeKind::Link, VOLUME), PathBuf::from(target))
        });
        self
    }

    /// `alias` names the same object as `existing` (same `FileId`), as `/SYSTEM` does `/System`.
    /// The folders above `alias` need not exist: `/.vol/1/2` names a file although
    /// `/.vol/1` is missing.
    pub fn alias(self, alias: &str, existing: &str) -> Self {
        self.tree().aliases.insert(
            abs(alias).as_path().to_path_buf(),
            abs(existing).as_path().to_path_buf(),
        );
        self
    }

    /// A directory that is the root of another volume.
    pub fn mount(self, path: &str, volume: u64) -> Self {
        self.tree()
            .put(path, |t| Entry::Node(t.fresh(NodeKind::Dir, volume)));
        self
    }

    /// `lstat` of `path`, and of everything below it, answers `Unknown(Denied)`.
    pub fn unreadable(self, path: &str) -> Self {
        self.tree().put(path, |_| Entry::Unreadable);
        self
    }

    pub fn running(mut self, pid: u32, exe: &str) -> Self {
        if let Seen::Found(table) = &mut self.processes {
            table.procs.push(Proc {
                pid: Pid(pid),
                exe: OsString::from(exe),
                args: OsString::new(),
            });
        }
        self
    }

    /// The table lists some processes but could not see all of them.
    pub fn process_table_incomplete(mut self) -> Self {
        if let Seen::Found(table) = &mut self.processes {
            table.complete = false;
        }
        self
    }

    pub fn process_table_unknown(self) -> Self {
        Self {
            processes: Seen::Unknown(Unseen::Denied),
            ..self
        }
    }

    pub fn open_file(mut self, path: &str) -> Self {
        self.open.insert(abs(path).as_path().to_path_buf());
        self
    }

    pub fn in_use_unknown(self) -> Self {
        Self {
            in_use_unknown: true,
            ..self
        }
    }

    /// Runs [`FakeProbe::swap`] on `path` in the middle of the next process-table probe, as an
    /// attacker who waits for the owner probe would.
    pub fn swap_during_next_process_probe(&self, path: &str) {
        *self
            .swap_on_process_probe
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(abs(path).as_path().to_path_buf());
    }

    /// Replaces the object at `path` with a new one of the same kind (new `FileId`), as a
    /// rename-and-recreate does between discovery and the sink.
    pub fn swap(&self, path: &str) {
        self.swap_at(abs(path).as_path());
    }

    fn swap_at(&self, path: &Path) {
        let mut tree = self.tree();
        let Ok(key) = tree.locate(path, false, 0) else {
            panic!("fixture {} does not exist", path.display());
        };
        let kind = match tree.nodes.get(&key) {
            Some(Entry::Node(node) | Entry::Link(node, _)) => node.kind,
            _ => panic!("fixture {} cannot be swapped", path.display()),
        };
        let node = tree.fresh(kind, VOLUME);
        tree.nodes.insert(key, Entry::Node(node));
    }

    /// Replaces the entry at `path` with a symlink to `target`.
    pub fn replace_with_symlink(&self, path: &str, target: &str) {
        let mut tree = self.tree();
        let node = tree.fresh(NodeKind::Link, VOLUME);
        tree.nodes.insert(
            abs(path).as_path().to_path_buf(),
            Entry::Link(node, PathBuf::from(target)),
        );
    }

    pub fn remove(&self, path: &str) {
        self.tree().nodes.remove(abs(path).as_path());
    }

    /// `lstat` calls so far. A test of ancestor sharing asserts on this, not on internals.
    pub fn node_calls(&self) -> usize {
        self.node_calls.load(Ordering::Relaxed)
    }

    pub fn process_calls(&self) -> usize {
        self.process_calls.load(Ordering::Relaxed)
    }
}

impl Default for FakeProbe {
    fn default() -> Self {
        Self::new()
    }
}

impl Probe for FakeProbe {
    fn node(&self, path: &AbsPath) -> Seen<Node> {
        self.node_calls.fetch_add(1, Ordering::Relaxed);
        let tree = self.tree();
        match tree.locate(path.as_path(), false, 0) {
            Err(Lost::Missing) => Seen::Missing,
            Err(Lost::Unknown(why)) => Seen::Unknown(why),
            Ok(key) => match tree.nodes.get(&key) {
                Some(Entry::Node(node) | Entry::Link(node, _)) => Seen::Found(*node),
                Some(Entry::Unreadable) => Seen::Unknown(Unseen::Denied),
                None => Seen::Missing,
            },
        }
    }

    fn link_target(&self, link: &AbsPath) -> Seen<PathBuf> {
        let tree = self.tree();
        match tree.locate(link.as_path(), false, 0) {
            Err(Lost::Missing) => Seen::Missing,
            Err(Lost::Unknown(why)) => Seen::Unknown(why),
            Ok(key) => match tree.nodes.get(&key) {
                Some(Entry::Link(_, target)) => Seen::Found(target.clone()),
                Some(_) => Seen::Unknown(Unseen::Failed),
                None => Seen::Missing,
            },
        }
    }

    fn spelling(&self, path: &AbsPath) -> Seen<AbsPath> {
        let tree = self.tree();
        match tree.locate(path.as_path(), false, 0) {
            Err(Lost::Missing) => Seen::Missing,
            Err(Lost::Unknown(why)) => Seen::Unknown(why),
            Ok(key) => match tree.nodes.get(&key) {
                Some(Entry::Node(_) | Entry::Link(..)) => Seen::Found(AbsPath::trusted(key)),
                Some(Entry::Unreadable) => Seen::Unknown(Unseen::Denied),
                None => Seen::Missing,
            },
        }
    }

    fn processes(&self) -> Seen<ProcessTable> {
        self.process_calls.fetch_add(1, Ordering::Relaxed);
        let swap = self
            .swap_on_process_probe
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        if let Some(path) = swap {
            self.swap_at(&path);
        }
        self.processes.clone()
    }

    fn in_use(&self, files: &[AbsPath]) -> Handles {
        if self.in_use_unknown {
            return Handles::Unknown(Unseen::Denied);
        }
        // Like a case-insensitive volume, where `X.DB-wal` and `X.DB-WAL` are one file.
        let same = |open: &PathBuf, file: &AbsPath| {
            let (open, file) = (open.as_os_str(), file.as_path().as_os_str());
            open.eq_ignore_ascii_case(file)
        };
        if files.iter().any(|f| self.open.iter().any(|o| same(o, f))) {
            Handles::Open
        } else {
            Handles::Closed
        }
    }
}
