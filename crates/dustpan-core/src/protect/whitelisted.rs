//! Layer 5: the whitelist compiled for one OS. Entries form a trie walked one component at a
//! time, like the protection rows, so a path costs a few hash lookups however many entries
//! there are.
//!
//! An entry keeps three things: the path it names and everything inside it, found when the walk
//! reaches an entry's last component before the path ends, and the folders above it, found when
//! the path ends first. A sibling that only shares a prefix (`foobar` against `foo`) differs in
//! a whole component, so it matches neither.
use super::names::{Comp, Glob, fold};
use crate::guard::{Reach, WhitelistHit};
use crate::path::AbsPath;
use crate::whitelist::{Entry, Origin, Pattern, Whitelist};
use std::collections::HashMap;
use std::ffi::OsStr;

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
    /// The first entry that ends at this node.
    ends: Option<u32>,
    /// The first entry that ends below this node.
    below: Option<u32>,
}

impl Whitelisted {
    pub(crate) fn compile(whitelist: &Whitelist, fold_case: bool) -> Self {
        let mut this = Self {
            fold_case,
            file: whitelist.file().cloned(),
            entries: whitelist.entries().to_vec(),
            nodes: vec![Node::default()],
            finder: None,
        };
        for (index, entry) in whitelist.entries().iter().enumerate() {
            let index = index as u32;
            match &entry.pattern {
                Pattern::FinderMetadata => {
                    this.finder.get_or_insert(index);
                }
                Pattern::Path(path) => this.insert(path, index),
            }
        }
        this
    }

    fn insert(&mut self, path: &AbsPath, entry: u32) {
        let mut at = 0;
        for name in path.names() {
            let node = &mut self.nodes[at as usize];
            node.below.get_or_insert(entry);
            let next = self.nodes.len() as u32;
            let node = &mut self.nodes[at as usize];
            at = match Comp::new(name.as_encoded_bytes(), self.fold_case) {
                Comp::Lit(key) => *node.exact.entry(key).or_insert(next),
                Comp::Glob(glob) => match node.globs.iter().find(|(g, _)| g.0 == glob.0) {
                    Some(&(_, child)) => child,
                    None => {
                        node.globs.push((glob, next));
                        next
                    }
                },
            };
            if at == next {
                self.nodes.push(Node::default());
            }
        }
        self.nodes[at as usize].ends.get_or_insert(entry);
    }

    pub(crate) fn hit(&self, path: &AbsPath, scratch: &mut Vec<u8>) -> Option<WhitelistHit> {
        if let Some(entry) = self.finder
            && path.name().is_some_and(|n| self.is_finder_file(n))
        {
            return Some(self.hit_of(entry, Reach::Within));
        }
        if let Some(entry) = self.nodes[0].ends {
            return Some(self.hit_of(entry, Reach::Within));
        }
        let mut frontier = vec![0u32];
        let mut next = Vec::new();
        for name in path.names() {
            let name = self.folded(name, scratch);
            next.clear();
            for &at in &frontier {
                let node = &self.nodes[at as usize];
                next.extend(node.exact.get(name).copied());
                next.extend(
                    node.globs
                        .iter()
                        .filter(|(g, _)| g.matches(name))
                        .map(|&(_, c)| c),
                );
            }
            if next.is_empty() {
                return None;
            }
            if let Some(entry) = next.iter().find_map(|&n| self.nodes[n as usize].ends) {
                return Some(self.hit_of(entry, Reach::Within));
            }
            std::mem::swap(&mut frontier, &mut next);
        }
        let entry = frontier
            .iter()
            .find_map(|&n| self.nodes[n as usize].below)?;
        Some(self.hit_of(entry, Reach::Above))
    }

    fn folded<'s>(&self, name: &OsStr, scratch: &'s mut Vec<u8>) -> &'s [u8] {
        scratch.clear();
        scratch.extend_from_slice(&fold(name.as_encoded_bytes(), self.fold_case));
        scratch
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
