use crate::guard::{PolicyError, Reach, WhitelistHit};
use crate::identity::NodeKind;
use crate::path::AbsPath;
use crate::probe::{self, Probe, Seen};
use crate::whitelist::glob::{Glob, fold};
use crate::whitelist::{Entry, Origin, Pattern, Whitelist};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::PathBuf;

pub(crate) struct Whitelisted {
    fold_case: bool,
    file: Option<AbsPath>,
    entries: Vec<Entry>,
    nodes: Vec<Node>,
    finder: Option<u32>,
}

#[derive(Default)]
struct Node {
    exact: HashMap<Box<[u8]>, u32>,
    globs: Vec<(Glob, u32)>,
    ends: Option<u32>,
    below: Option<u32>,
}

#[derive(Clone)]
pub(crate) enum Position {
    Outside,
    Inside(u32),
    At(Box<[u32]>),
}

impl Whitelisted {
    pub(crate) fn empty(fold_case: bool) -> Self {
        Self {
            fold_case,
            file: None,
            entries: Vec::new(),
            nodes: vec![Node::default()],
            finder: None,
        }
    }

    /// Each entry goes in as written and as the file system spells it, both under `canonical`'s
    /// mapping of firmlinks, so every spelling the guard can judge meets the entry.
    pub(crate) fn compile(
        whitelist: &Whitelist,
        fold_case: bool,
        canonical: impl Fn(&AbsPath) -> AbsPath,
        probe: &dyn Probe,
    ) -> Result<Self, PolicyError> {
        let mut this = Self {
            file: whitelist.file().cloned(),
            entries: whitelist.entries().to_vec(),
            ..Self::empty(fold_case)
        };
        let mut resolver = Resolver {
            probe,
            physical: HashMap::new(),
            stored: HashMap::new(),
        };
        for (index, entry) in whitelist.entries().iter().enumerate() {
            let index = index as u32;
            match &entry.pattern {
                Pattern::FinderMetadata => {
                    this.finder.get_or_insert(index);
                }
                Pattern::Path(path) => {
                    let written = canonical(path);
                    let (root, names) = split_root(&written);
                    this.insert(&root, names, index);
                    let (found, depth) = resolver.resolve(path)?;
                    let (_, names) = split_root(path);
                    this.insert(&canonical(&found), names.skip(depth), index);
                }
            }
        }
        Ok(this)
    }

    /// Adds `head`'s names as they are, since a folder on disk may hold `*` or `[` in its name,
    /// then `tail`'s as the entry wrote them.
    fn insert<'n>(&mut self, head: &AbsPath, tail: impl Iterator<Item = &'n OsStr>, entry: u32) {
        let mut at = 0;
        let fold_case = self.fold_case;
        let head = head.names().map(|name| (name, None));
        let tail = tail.map(|name| {
            let glob = Glob::parse(name, fold_case).expect("load checked every entry's names");
            (name, glob)
        });
        for (name, glob) in head.chain(tail) {
            self.nodes[at as usize].below.get_or_insert(entry);
            let new = self.nodes.len() as u32;
            let node = &mut self.nodes[at as usize];
            at = match glob {
                None => {
                    let mut key = Vec::new();
                    fold(name, self.fold_case, &mut key);
                    *node.exact.entry(key.into()).or_insert(new)
                }
                Some(glob) => match node.globs.iter().find(|(g, _)| g.same(&glob)) {
                    Some(&(_, child)) => child,
                    None => {
                        node.globs.push((glob, new));
                        new
                    }
                },
            };
            if at == new {
                self.nodes.push(Node::default());
            }
        }
        self.nodes[at as usize].ends.get_or_insert(entry);
    }

    pub(crate) fn start(&self, path: &AbsPath, scratch: &mut Vec<u8>) -> Position {
        let top = match self.nodes[0].ends {
            Some(entry) => Position::Inside(entry),
            None => Position::At(Box::new([0])),
        };
        path.names()
            .fold(top, |at, name| self.step(&at, name, scratch))
    }

    pub(crate) fn step(&self, from: &Position, name: &OsStr, scratch: &mut Vec<u8>) -> Position {
        let Position::At(nodes) = from else {
            return from.clone();
        };
        fold(name, self.fold_case, scratch);
        let mut next = Vec::new();
        for child in self.children(nodes, scratch) {
            if let Some(entry) = self.nodes[child as usize].ends {
                return Position::Inside(entry);
            }
            next.push(child);
        }
        if next.is_empty() {
            Position::Outside
        } else {
            Position::At(next.into())
        }
    }

    pub(crate) fn hit(
        &self,
        from: &Position,
        name: &OsStr,
        scratch: &mut Vec<u8>,
    ) -> Option<WhitelistHit> {
        if let Some(entry) = self.finder
            && self.is_finder_file(name)
        {
            return Some(self.hit_of(entry, Reach::Within));
        }
        match from {
            Position::Outside => None,
            Position::Inside(entry) => Some(self.hit_of(*entry, Reach::Within)),
            Position::At(nodes) => {
                fold(name, self.fold_case, scratch);
                let mut above = None;
                for child in self.children(nodes, scratch) {
                    let node = &self.nodes[child as usize];
                    if let Some(entry) = node.ends {
                        return Some(self.hit_of(entry, Reach::Within));
                    }
                    above = above.or(node.below);
                }
                above.map(|entry| self.hit_of(entry, Reach::Above))
            }
        }
    }

    pub(crate) fn here(&self, at: &Position) -> Option<WhitelistHit> {
        match at {
            Position::Outside => None,
            Position::Inside(entry) => Some(self.hit_of(*entry, Reach::Within)),
            Position::At(nodes) => nodes
                .iter()
                .find_map(|&node| self.nodes[node as usize].below)
                .map(|entry| self.hit_of(entry, Reach::Above)),
        }
    }

    /// `name` is a key from [`fold`].
    fn children<'a>(&'a self, nodes: &'a [u32], name: &'a [u8]) -> impl Iterator<Item = u32> + 'a {
        nodes.iter().flat_map(move |&at| {
            let node = &self.nodes[at as usize];
            node.exact.get(name).copied().into_iter().chain(
                node.globs
                    .iter()
                    .filter(move |(glob, _)| glob.matches(name))
                    .map(|&(_, child)| child),
            )
        })
    }

    fn is_finder_file(&self, name: &OsStr) -> bool {
        let name = name.as_encoded_bytes();
        if self.fold_case {
            name.eq_ignore_ascii_case(b".DS_Store")
        } else {
            name == b".DS_Store"
        }
    }

    fn hit_of(&self, entry: u32, reach: Reach) -> WhitelistHit {
        let Entry { pattern, origin } = &self.entries[entry as usize];
        WhitelistHit {
            origin: *origin,
            file: self.file.clone().filter(|_| *origin == Origin::User),
            entry: pattern.clone(),
            reach,
        }
    }
}

/// Finds where each entry lies on disk, asking once per folder the entries share.
struct Resolver<'p> {
    probe: &'p dyn Probe,
    /// A run of leading names as written, and the physical folder it leads to (`None` when it
    /// does not exist).
    physical: HashMap<PathBuf, Option<AbsPath>>,
    stored: HashMap<AbsPath, AbsPath>,
}

/// The root of `path` (`/` or a drive's `C:\`), and the names below it.
fn split_root(path: &AbsPath) -> (AbsPath, std::path::Iter<'_>) {
    let path = path.as_path();
    let root = path.ancestors().last().unwrap_or(path);
    let names = path.strip_prefix(root).unwrap_or(path).iter();
    (AbsPath::trusted(root.to_path_buf()), names)
}

impl Resolver<'_> {
    /// Where `entry` lies on disk, and how many of its names below the root that covers. The
    /// longest run of its leading names with no wildcard that exists is followed through every
    /// link, the last one included, and spelled as stored. An entry that names a link therefore
    /// also keeps what the link points at, which is where everything inside it lies.
    fn resolve(&mut self, entry: &AbsPath) -> Result<(AbsPath, usize), PolicyError> {
        let (mut found, names) = split_root(entry);
        let mut written = found.as_path().to_path_buf();
        let mut depth = 0;
        for name in names.take_while(|&name| matches!(Glob::parse(name, false), Ok(None))) {
            written.push(name);
            let next = match self.physical.get(&written) {
                Some(known) => known.clone(),
                None => {
                    let seen = self.step(&found, name, entry)?;
                    self.physical.insert(written.clone(), seen.clone());
                    seen
                }
            };
            let Some(next) = next else {
                break;
            };
            found = next;
            depth += 1;
        }
        let spelled = match depth {
            0 => found,
            _ => self.stored(found),
        };
        Ok((spelled, depth))
    }

    fn step(
        &self,
        parent: &AbsPath,
        name: &OsStr,
        entry: &AbsPath,
    ) -> Result<Option<AbsPath>, PolicyError> {
        let next = AbsPath::trusted(parent.as_path().join(name));
        let seen = match self.probe.node(&next) {
            Seen::Found(node) if node.kind == NodeKind::Link => probe::physical(self.probe, &next),
            Seen::Found(_) => Seen::Found(next),
            Seen::Missing => Seen::Missing,
            Seen::Unknown(why) => Seen::Unknown(why),
        };
        match seen {
            Seen::Found(next) => Ok(Some(next)),
            Seen::Missing => Ok(None),
            Seen::Unknown(why) => Err(PolicyError::AnchorUnreadable {
                anchor: entry.clone(),
                why,
            }),
        }
    }

    /// The stored spelling of a physical path. When the probe cannot say, the physical path
    /// stands: folding already meets its case and normalization aliases.
    fn stored(&mut self, physical: AbsPath) -> AbsPath {
        if let Some(known) = self.stored.get(&physical) {
            return known.clone();
        }
        let spelled = match self.probe.spelling(&physical) {
            Seen::Found(spelled) => spelled,
            Seen::Missing | Seen::Unknown(_) => physical.clone(),
        };
        self.stored.insert(physical, spelled.clone());
        spelled
    }
}
