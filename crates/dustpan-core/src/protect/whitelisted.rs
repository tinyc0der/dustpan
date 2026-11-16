use super::names::{Comp, Glob};
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
            self.nodes[at as usize].below.get_or_insert(entry);
            let new = self.nodes.len() as u32;
            let node = &mut self.nodes[at as usize];
            at = match Comp::new(name.as_encoded_bytes(), self.fold_case) {
                Comp::Lit(key) => *node.exact.entry(key).or_insert(new),
                Comp::Glob(glob) => match node.globs.iter().find(|(g, _)| g.0 == glob.0) {
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
        let name = self.folded(name, scratch);
        let mut next = Vec::new();
        for child in self.children(nodes, name) {
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
                let name = self.folded(name, scratch);
                let mut above = None;
                for child in self.children(nodes, name) {
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

    fn folded<'s>(&self, name: &OsStr, scratch: &'s mut Vec<u8>) -> &'s [u8] {
        scratch.clear();
        scratch.extend(name.as_encoded_bytes().iter().map(|&b| {
            if self.fold_case {
                b.to_ascii_lowercase()
            } else {
                b
            }
        }));
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
