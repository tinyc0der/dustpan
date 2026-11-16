//! An in-memory file system, process table and set of open files for core tests. Compiled only for
//! core's own unit tests and with the `testkit` feature, which no shipped crate enables; a source
//! test fails if `FakeProbe` or `testkit` appears under `crates/dustpan/src` or
//! `crates/dustpan-sys/src` (SAFE-17).
use super::{App, OpenFile, OpenFiles, Pid, Probe, Proc, ProcessTable, Seen, Unlisted, Unseen};
use crate::disk::{Disk, DiskError, Seal, Trashed, Unlink};
use crate::identity::{DirEntry, FileId, Node, NodeKind};
use crate::path::AbsPath;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

const VOLUME: u64 = 1;
const MAX_LINK_HOPS: usize = 40;

/// Builder-style fake. Paths are in host syntax. Two spellings that share a `FileId` are
/// aliases, which is how a test models case-insensitive APFS (`/SYSTEM`) or a firmlink. Like a
/// kernel, it follows links and aliases in every component but the last.
///
/// The same tree serves the guard (`Probe`) and the sinks (`Disk`), so a test that swaps an
/// inode, replaces a parent with a link or mounts a volume inside a candidate between the
/// recheck and the sink sees both sides of the race.
pub struct FakeProbe {
    tree: Mutex<Tree>,
    processes: Seen<ProcessTable>,
    open: Mutex<Seen<OpenFiles>>,
    swap_on_process_probe: Mutex<Option<PathBuf>>,
    before_sink: Mutex<Option<Change>>,
    trash_unavailable: bool,
    node_calls: AtomicUsize,
    process_calls: AtomicUsize,
    open_file_calls: AtomicUsize,
}

type Change = Box<dyn FnOnce(&FakeProbe) + Send>;

/// Each file the fake lists takes one 4 KiB block.
const BLOCK: u64 = 4096;

#[derive(Default)]
struct Tree {
    nodes: BTreeMap<PathBuf, Entry>,
    aliases: BTreeMap<PathBuf, PathBuf>,
    next_id: u128,
    dataless: BTreeSet<PathBuf>,
    mount_roots: BTreeSet<PathBuf>,
    removed: Vec<PathBuf>,
    trashed: Vec<PathBuf>,
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

    /// What a directory read reports for the entry at `key`. A folder's link count is the
    /// conventional 2; a file's counts every name that shares its `FileId`.
    fn dir_entry(&self, key: &Path) -> Result<DirEntry, DiskError> {
        let node = match self.nodes.get(key) {
            Some(Entry::Node(node) | Entry::Link(node, _)) => *node,
            // A folder its owner closed to us: its parent lists it, opening it fails.
            Some(Entry::Unreadable) => Node {
                kind: NodeKind::Dir,
                id: FileId::new(VOLUME, u128::MAX - key.as_os_str().len() as u128),
            },
            None => return Err(DiskError::NotFound),
        };
        let links = match node.kind {
            NodeKind::Dir => 2,
            _ => self
                .nodes
                .values()
                .filter(|e| matches!(e, Entry::Node(n) | Entry::Link(n, _) if n.id == node.id))
                .count(),
        };
        Ok(DirEntry {
            name: key.file_name().unwrap_or_default().to_os_string(),
            node,
            alloc: if node.kind == NodeKind::File {
                BLOCK
            } else {
                0
            },
            links: u32::try_from(links).unwrap_or(u32::MAX),
            mount_root: self.mount_roots.contains(key),
            dataless: self.dataless.contains(key),
        })
    }

    fn children(&self, key: &Path) -> impl Iterator<Item = &PathBuf> {
        self.nodes
            .range(key.to_path_buf()..)
            .map(|(path, _)| path)
            .take_while(move |path| path.starts_with(key))
            .filter(move |path| path.parent() == Some(key))
    }

    /// Every entry of the folder at `key`, as `getdents` plus `fstatat` would report them.
    fn list(&self, key: &Path, out: &mut Vec<DirEntry>) -> Result<(), DiskError> {
        for child in self.children(key) {
            out.push(self.dir_entry(child)?);
        }
        Ok(())
    }

    /// `open(2)` with `O_NOFOLLOW_ANY | O_DIRECTORY`: a link or alias anywhere fails.
    fn open_nofollow(&self, path: &Path) -> Result<FakeDir, DiskError> {
        let key = match self.locate(path, false, 0) {
            Ok(key) => key,
            Err(Lost::Missing) => return Err(DiskError::NotFound),
            Err(Lost::Unknown(Unseen::Denied)) => return Err(DiskError::Denied),
            Err(Lost::Unknown(_)) => return Err(DiskError::NotAFolder),
        };
        if key != path {
            return Err(DiskError::NotAFolder);
        }
        self.open_at(key)
    }

    fn open_at(&self, key: PathBuf) -> Result<FakeDir, DiskError> {
        match self.nodes.get(&key) {
            Some(Entry::Node(node)) if node.kind == NodeKind::Dir => {
                Ok(FakeDir { id: node.id, key })
            }
            Some(Entry::Node(_) | Entry::Link(..)) => Err(DiskError::NotAFolder),
            Some(Entry::Unreadable) => Err(DiskError::Denied),
            None => Err(DiskError::NotFound),
        }
    }

    /// The key a handle names while its folder is still there. A real handle follows its
    /// folder anywhere; the fake models one whose folder was replaced as detached.
    fn held<'a>(&self, dir: &'a FakeDir) -> Result<&'a Path, DiskError> {
        match self.nodes.get(&dir.key) {
            Some(Entry::Node(node)) if node.id == dir.id => Ok(&dir.key),
            _ => Err(DiskError::NotFound),
        }
    }

    /// Removes `key` and everything below it.
    fn drop_tree(&mut self, key: &Path) {
        self.nodes.retain(|path, _| !path.starts_with(key));
    }
}

/// An open folder of the fake: the key it was opened at and the object it was then.
pub struct FakeDir {
    key: PathBuf,
    id: FileId,
}

impl FakeProbe {
    /// Holds the root folder on Unix, an empty, complete process table and no open file.
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
            open: Mutex::new(Seen::Found(OpenFiles::default())),
            swap_on_process_probe: Mutex::new(None),
            before_sink: Mutex::new(None),
            trash_unavailable: false,
            node_calls: AtomicUsize::new(0),
            process_calls: AtomicUsize::new(0),
            open_file_calls: AtomicUsize::new(0),
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

    /// A file whose name `AbsPath::parse` would refuse, such as one holding a newline. A
    /// directory can hold one, so a listing returns it.
    pub fn file_in(self, dir: &str, name: &str) -> Self {
        {
            let mut tree = self.tree();
            tree.ensure_dirs(&abs(dir));
            let node = tree.fresh(NodeKind::File, VOLUME);
            tree.nodes
                .insert(abs(dir).as_path().join(name), Entry::Node(node));
        }
        self
    }

    /// The path of `name` in `dir` as a directory read would form it, control bytes included.
    pub fn listed(&self, dir: &str, name: &str) -> AbsPath {
        AbsPath::trusted(abs(dir).as_path().join(name))
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

    /// A folder on the same volume that is the root of a mount, as a Linux bind mount is.
    pub fn bind_mount(self, path: &str) -> Self {
        let dir = self.dir(path);
        dir.tree()
            .mount_roots
            .insert(abs(path).as_path().to_path_buf());
        dir
    }

    /// An online-only file: listed with its dataless flag, never read.
    pub fn dataless(self, path: &str) -> Self {
        let file = self.file(path);
        file.tree()
            .dataless
            .insert(abs(path).as_path().to_path_buf());
        file
    }

    /// `path` is another name for the file at `existing` (same `FileId`).
    pub fn hard_link(self, path: &str, existing: &str) -> Self {
        {
            let mut tree = self.tree();
            let node = match tree.nodes.get(abs(existing).as_path()) {
                Some(Entry::Node(node)) if node.kind == NodeKind::File => *node,
                _ => panic!("fixture {existing} is not a file"),
            };
            tree.put(path, |_| Entry::Node(node));
        }
        self
    }

    /// `lstat` of `path`, and of everything below it, answers `Unknown(Denied)`.
    pub fn unreadable(self, path: &str) -> Self {
        self.tree().put(path, |_| Entry::Unreadable);
        self
    }

    /// A process with a bare executable: in no bundle.
    pub fn running(self, pid: u32, exe: &str) -> Self {
        self.process(pid, exe, App::bare())
    }

    /// A process whose executable sits in the bundle `bundle_id`.
    pub fn running_app(self, pid: u32, exe: &str, bundle_id: &str) -> Self {
        self.process(pid, exe, App::Bundles(Box::new([bundle_id.into()])))
    }

    /// A process the probe could not identify, such as one whose executable was deleted. Only
    /// its short name is known. `mine` says it runs as the user dustpan runs as.
    pub fn running_unidentified(self, pid: u32, short_name: &str, mine: bool) -> Self {
        self.process(pid, short_name, App::Unknown { mine })
    }

    fn process(mut self, pid: u32, exe: &str, app: App) -> Self {
        if let Seen::Found(table) = &mut self.processes {
            table.procs.push(Proc {
                pid: Pid(pid),
                exe: OsString::from(exe),
                args: OsString::new(),
                app,
                signing_id: None,
                app_groups: Box::default(),
            });
        }
        self
    }

    /// Gives the process added last a code signature: its identifier, and the app groups its
    /// entitlements name.
    pub fn signed(mut self, signing_id: &str, app_groups: &[&str]) -> Self {
        if let Seen::Found(table) = &mut self.processes
            && let Some(proc) = table.procs.last_mut()
        {
            proc.signing_id = Some(signing_id.into());
            proc.app_groups = app_groups.iter().map(|&group| group.into()).collect();
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

    fn open(&self) -> std::sync::MutexGuard<'_, Seen<OpenFiles>> {
        self.open.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Process 1 holds `path` open.
    pub fn open_file(self, path: &str) -> Self {
        self.open_now(1, path);
        self
    }

    pub fn open_by(self, pid: u32, path: &str) -> Self {
        self.open_now(pid, path);
        self
    }

    /// Process `pid` opens `path` from now on, as an app does between discovery and the sink.
    pub fn open_now(&self, pid: u32, path: &str) {
        if let Seen::Found(open) = &mut *self.open() {
            open.files.push(OpenFile {
                pid: Pid(pid),
                path: abs(path).as_path().to_path_buf(),
            });
        }
    }

    pub fn close_all(&self) {
        if let Seen::Found(open) = &mut *self.open() {
            open.files.clear();
        }
    }

    /// Process `pid` runs and does not let the probe list its open files. `mine` says it runs
    /// as the user dustpan runs as.
    pub fn unlisted(self, pid: u32, mine: bool) -> Self {
        self.unlisted_now(pid, mine);
        self
    }

    /// The probe cannot list the open files of process `pid` from now on, as when the process
    /// opens a file whose path the probe cannot confirm between discovery and the sink.
    pub fn unlisted_now(&self, pid: u32, mine: bool) {
        if let Seen::Found(open) = &mut *self.open() {
            open.unlisted.push(Unlisted {
                pid: Pid(pid),
                mine,
            });
        }
    }

    /// The snapshot of open files cannot be taken at all.
    pub fn in_use_unknown(self) -> Self {
        *self.open() = Seen::Unknown(Unseen::Denied);
        self
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
        self.tree().drop_tree(abs(path).as_path());
    }

    /// Runs `change` the first time a sink opens a parent, which is after `recheck` approved:
    /// an inode swapped, a parent replaced by a link, a mount appearing.
    pub fn before_sink(&self, change: impl FnOnce(&FakeProbe) + Send + 'static) {
        *self.before_sink.lock().unwrap_or_else(|e| e.into_inner()) = Some(Box::new(change));
    }

    /// Every Trash move fails with `NoTrash`, as with no Trash on the volume (SAFE-12).
    pub fn trash_unavailable(self) -> Self {
        Self {
            trash_unavailable: true,
            ..self
        }
    }

    /// Paths a sink unlinked, in order.
    pub fn removed(&self) -> Vec<PathBuf> {
        self.tree().removed.clone()
    }

    /// Paths a sink moved to the Trash, in order.
    pub fn trashed(&self) -> Vec<PathBuf> {
        self.tree().trashed.clone()
    }

    /// Whether anything is at `path`, without counting an `lstat`.
    pub fn exists(&self, path: &str) -> bool {
        self.tree().nodes.contains_key(abs(path).as_path())
    }

    /// `lstat` calls so far. A test of ancestor sharing asserts on this, not on internals.
    pub fn node_calls(&self) -> usize {
        self.node_calls.load(Ordering::Relaxed)
    }

    pub fn process_calls(&self) -> usize {
        self.process_calls.load(Ordering::Relaxed)
    }

    pub fn open_file_calls(&self) -> usize {
        self.open_file_calls.load(Ordering::Relaxed)
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

    fn open_files(&self) -> Seen<OpenFiles> {
        self.open_file_calls.fetch_add(1, Ordering::Relaxed);
        self.open().clone()
    }

    fn list(&self, dir: &AbsPath, out: &mut Vec<DirEntry>) -> Seen<FileId> {
        let tree = self.tree();
        let opened = tree.open_nofollow(dir.as_path()).and_then(|handle| {
            tree.list(&handle.key, out)?;
            Ok(handle.id)
        });
        match opened {
            Ok(id) => Seen::Found(id),
            Err(DiskError::NotFound) => Seen::Missing,
            Err(DiskError::Denied) => Seen::Unknown(Unseen::Denied),
            Err(_) => Seen::Unknown(Unseen::Failed),
        }
    }
}

impl Disk for FakeProbe {
    type Dir = FakeDir;

    fn open_parent(&self, path: &AbsPath, _: &Seal) -> Result<FakeDir, DiskError> {
        let change = self
            .before_sink
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        if let Some(change) = change {
            change(self);
        }
        let parent = path.parent().ok_or(DiskError::NotAFolder)?;
        self.tree().open_nofollow(parent.as_path())
    }

    fn open_dir(&self, at: &FakeDir, name: &OsStr, _: &Seal) -> Result<FakeDir, DiskError> {
        let tree = self.tree();
        let key = tree.held(at)?.join(name);
        tree.open_at(key)
    }

    fn id(&self, dir: &FakeDir, _: &Seal) -> Result<FileId, DiskError> {
        Ok(dir.id)
    }

    fn stat(&self, at: &FakeDir, name: &OsStr, _: &Seal) -> Result<DirEntry, DiskError> {
        let tree = self.tree();
        tree.dir_entry(&tree.held(at)?.join(name))
    }

    fn list(&self, dir: &FakeDir, out: &mut Vec<DirEntry>, _: &Seal) -> Result<(), DiskError> {
        let tree = self.tree();
        tree.list(tree.held(dir)?, out)
    }

    fn unlink(&self, at: &FakeDir, name: &OsStr, what: Unlink, _: &Seal) -> Result<(), DiskError> {
        let mut tree = self.tree();
        let key = tree.held(at)?.join(name);
        let entry = tree.dir_entry(&key)?;
        match (entry.node.kind, what) {
            (NodeKind::Dir, Unlink::NotDir) => return Err(DiskError::Denied),
            (NodeKind::Dir, Unlink::Dir) => {
                if entry.mount_root || entry.node.id.volume() != at.id.volume() {
                    return Err(DiskError::Busy);
                }
                if tree.children(&key).next().is_some() {
                    return Err(DiskError::NotEmpty);
                }
            }
            (_, Unlink::Dir) => return Err(DiskError::NotAFolder),
            (_, Unlink::NotDir) => {}
        }
        tree.nodes.remove(&key);
        tree.removed.push(key);
        Ok(())
    }

    fn trash(
        &self,
        at: &FakeDir,
        path: &AbsPath,
        expect: FileId,
        _: &Seal,
    ) -> Result<Trashed, DiskError> {
        if self.trash_unavailable {
            return Err(DiskError::NoTrash);
        }
        let mut tree = self.tree();
        let name = path.name().ok_or(DiskError::NotAFolder)?;
        let key = tree.held(at)?.join(name);
        let arrived = tree.dir_entry(&key)?.node.id;
        if arrived != expect {
            return Err(DiskError::NotAFolder);
        }
        tree.drop_tree(&key);
        tree.trashed.push(key);
        Ok(Trashed {
            at: Path::new("/.Trash").join(name),
            arrived,
        })
    }
}
