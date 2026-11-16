//! Matching one name: globs, bundle-id patterns and keywords.
use crate::guard::{Kind, PolicyError};
use std::collections::HashSet;

pub(super) fn fold(bytes: &[u8], fold_case: bool) -> Vec<u8> {
    if fold_case {
        bytes.to_ascii_lowercase()
    } else {
        bytes.to_vec()
    }
}

pub(super) fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

/// One component of a row's anchor.
pub(super) enum Comp {
    Lit(Box<[u8]>),
    Glob(Glob),
}

impl Comp {
    pub(super) fn new(name: &[u8], fold_case: bool) -> Self {
        let folded = fold(name, fold_case);
        if folded.contains(&b'*') {
            Self::Glob(Glob(folded.into()))
        } else {
            Self::Lit(folded.into())
        }
    }
}

/// A pattern over one name where `*` matches any run of bytes.
pub(super) struct Glob(pub Box<[u8]>);

impl Glob {
    pub(super) fn matches(&self, name: &[u8]) -> bool {
        self.matches_by(name, |b| b)
    }

    /// Matches `name` after `map` is applied to each of its bytes.
    fn matches_by(&self, name: &[u8], map: impl Fn(u8) -> u8) -> bool {
        let pattern = &*self.0;
        let (mut p, mut n) = (0, 0);
        let mut retry: Option<(usize, usize)> = None;
        while n < name.len() {
            if p < pattern.len() && pattern[p] == b'*' {
                retry = Some((p, n));
                p += 1;
            } else if p < pattern.len() && pattern[p] == map(name[n]) {
                p += 1;
                n += 1;
            } else if let Some((star, at)) = retry {
                retry = Some((star, at + 1));
                p = star + 1;
                n = at + 1;
            } else {
                return false;
            }
        }
        pattern[p..].iter().all(|&b| b == b'*')
    }
}

/// Bundle-id globs sorted by shape, so one name costs a hash lookup, a trie walk and a few
/// substring scans instead of five hundred glob matches.
#[derive(Default)]
pub(super) struct NamePatterns {
    exact: HashSet<Box<[u8]>>,
    prefixes: PrefixSet,
    suffixes: Vec<Box<[u8]>>,
    contains: Substrings,
    general: Vec<Glob>,
    /// `*a*b` patterns. Mole matches a pattern that starts with `*` against the whole path, and
    /// these can match across a separator, so they keep that meaning.
    whole_path: Vec<Glob>,
    /// The patterns are folded, and so is every name they are asked about.
    fold_case: bool,
    keywords: Vec<(Box<[u8]>, Kind)>,
}

pub(super) enum BundleMatch {
    No,
    /// The name itself is protected.
    Name,
    /// A `*x*` pattern: the name and everything below it are protected.
    Anywhere,
}

impl NamePatterns {
    /// A bundle id names one app on a case-insensitive volume however it is spelled, so the
    /// patterns are folded there. On a case-sensitive one they match as written.
    pub(super) fn new(fold_case: bool) -> Self {
        Self {
            fold_case,
            ..Self::default()
        }
    }

    pub(super) fn add_bundle(&mut self, pattern: &'static str) -> Result<(), PolicyError> {
        let folded = fold(pattern.as_bytes(), self.fold_case);
        let bytes = &folded[..];
        let trimmed = pattern.trim_matches('*').len();
        if trimmed == 0 {
            return Err(PolicyError::BadAnchor(pattern));
        }
        let start = bytes.iter().take_while(|&&b| b == b'*').count();
        let inner = &bytes[start..start + trimmed];
        let (lead, trail) = (bytes[0] == b'*', bytes[bytes.len() - 1] == b'*');
        match (inner.contains(&b'*'), lead, trail) {
            (true, true, _) => self.whole_path.push(Glob(bytes.into())),
            (true, false, _) => self.general.push(Glob(bytes.into())),
            (false, false, false) => drop(self.exact.insert(bytes.into())),
            (false, false, true) => self.prefixes.insert(inner),
            (false, true, false) => self.suffixes.push(inner.into()),
            (false, true, true) => self.contains.insert(inner),
        }
        Ok(())
    }

    pub(super) fn add_keyword(&mut self, folded: Vec<u8>, kind: Kind) {
        self.keywords.push((folded.into(), kind));
    }

    /// `name` is folded when the patterns are.
    pub(super) fn bundle(&self, name: &[u8]) -> BundleMatch {
        if self.contains.any_in(name) {
            BundleMatch::Anywhere
        } else if self.exact.contains(name)
            || self.prefixes.matches(name)
            || self.suffixes.iter().any(|s| name.ends_with(s))
            || self.general.iter().any(|g| g.matches(name))
        {
            BundleMatch::Name
        } else {
            BundleMatch::No
        }
    }

    /// `path` is as spelled; it is folded here, one byte at a time.
    pub(super) fn whole_path(&self, path: &[u8]) -> bool {
        let fold = |b: u8| {
            if self.fold_case {
                b.to_ascii_lowercase()
            } else {
                b
            }
        };
        self.whole_path.iter().any(|g| g.matches_by(path, fold))
    }

    pub(super) fn keyword(&self, folded: &[u8]) -> Option<Kind> {
        self.keywords
            .iter()
            .find(|(k, _)| contains(folded, k))
            .map(|&(_, kind)| kind)
    }
}

/// The fixed parts of `*x*` patterns, grouped by first byte. A name is scanned once, and a
/// position is compared only with the few patterns that start with the byte found there.
struct Substrings {
    by_first: [Vec<Box<[u8]>>; 256],
}

impl Default for Substrings {
    fn default() -> Self {
        Self {
            by_first: std::array::from_fn(|_| Vec::new()),
        }
    }
}

impl Substrings {
    fn insert(&mut self, needle: &[u8]) {
        self.by_first[usize::from(needle[0])].push(needle.into());
    }

    fn any_in(&self, hay: &[u8]) -> bool {
        (0..hay.len()).any(|at| {
            self.by_first[usize::from(hay[at])]
                .iter()
                .any(|needle| hay[at..].starts_with(needle))
        })
    }
}

/// Byte trie over the fixed prefixes of `prefix*` patterns.
struct PrefixSet {
    nodes: Vec<(Vec<(u8, u32)>, bool)>,
}

impl Default for PrefixSet {
    fn default() -> Self {
        Self {
            nodes: vec![(Vec::new(), false)],
        }
    }
}

impl PrefixSet {
    fn insert(&mut self, prefix: &[u8]) {
        let mut at = 0;
        for &b in prefix {
            let next = self.nodes[at].0.iter().find(|(k, _)| *k == b).map(|e| e.1);
            at = match next {
                Some(next) => next as usize,
                None => {
                    self.nodes.push((Vec::new(), false));
                    let new = self.nodes.len() - 1;
                    self.nodes[at].0.push((b, new as u32));
                    new
                }
            };
        }
        self.nodes[at].1 = true;
    }

    fn matches(&self, name: &[u8]) -> bool {
        let mut at = 0;
        for &b in name {
            match self.nodes[at].0.iter().find(|(k, _)| *k == b) {
                Some(&(_, next)) => at = next as usize,
                None => return false,
            }
            if self.nodes[at].1 {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_glob_matches_like_a_shell_star() {
        let glob = |p: &str| Glob(p.as_bytes().into());
        assert!(glob("*dev.orbstack*").matches(b"HUAQ24HBR6.dev.orbstack"));
        assert!(glob("com.apple.*").matches(b"com.apple.Safari"));
        assert!(!glob("com.apple.*").matches(b"com.applesauce"));
        assert!(glob("state_*.sqlite").matches(b"state_5.sqlite"));
        assert!(!glob("state_*.sqlite").matches(b"state_5.sqlite-wal"));
        assert!(glob("*").matches(b""));
        assert!(!glob("a*b").matches(b"ab-c"));
    }

    #[test]
    fn a_prefix_set_matches_any_listed_prefix() {
        let mut set = PrefixSet::default();
        set.insert(b"com.apple.");
        set.insert(b"keyboard");
        assert!(set.matches(b"com.apple.finder"));
        assert!(set.matches(b"keyboard"));
        assert!(!set.matches(b"com.apple"));
        assert!(!set.matches(b"keyboar"));
    }

    #[test]
    fn bundle_patterns_are_sorted_by_shape() {
        let mut names = NamePatterns::default();
        for pattern in [
            "Claude",
            "com.apple.*",
            "*.inputmethod",
            "*tailscale*",
            "a*b",
            "*x*y*",
        ] {
            names.add_bundle(pattern).unwrap();
        }
        assert!(matches!(names.bundle(b"Claude"), BundleMatch::Name));
        assert!(matches!(names.bundle(b"claude"), BundleMatch::No));
        assert!(matches!(
            names.bundle(b"com.apple.Safari"),
            BundleMatch::Name
        ));
        assert!(matches!(names.bundle(b"x.inputmethod"), BundleMatch::Name));
        assert!(matches!(
            names.bundle(b"io.tailscale.ipn"),
            BundleMatch::Anywhere
        ));
        assert!(matches!(names.bundle(b"a-to-b"), BundleMatch::Name));
        assert!(names.whole_path(b"/a/x/b/y/c"));
        assert!(!names.whole_path(b"/a/y/b/x"));
        assert!(names.add_bundle("*").is_err());
    }
}
