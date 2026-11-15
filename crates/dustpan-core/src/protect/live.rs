//! Layer 3's pure part: which paths need a live probe, decided from spelling alone.
use super::names::{Comp, contains, fold};
use super::table::Table;
use crate::guard::{GuardInputs, PolicyError};
use crate::path::AbsPath;
use std::ffi::OsString;
use std::path::MAIN_SEPARATOR;

const SEP: u8 = MAIN_SEPARATOR as u8;
const COMPANIONS: [&str; 3] = ["-wal", "-shm", "-journal"];

pub(super) struct LiveRules {
    fold_case: bool,
    home: Vec<u8>,
    scopes: Vec<Vec<Comp>>,
    powerlog: Option<Vec<u8>>,
    edr: Option<(Vec<u8>, Vec<Vec<u8>>)>,
    sqlite: &'static [&'static str],
    partial: Vec<(Vec<u8>, &'static [&'static str])>,
}

/// What layer 3 needs for one path.
#[derive(Default)]
pub(crate) struct LiveChecks<'p> {
    pub powerlog: bool,
    pub edr: bool,
    /// Bundle id of a user-cache scope, when the path is in one.
    pub scope_owner: Option<&'p str>,
    /// The SQLite family or partial download to ask `Probe::in_use` about.
    pub family: Option<Vec<AbsPath>>,
    /// A SQLite companion whose name cannot be split, so its family cannot be asked about.
    pub family_unreadable: bool,
}

/// What follows `dir` and one separator in `path`.
fn below<'a>(path: &'a [u8], dir: &[u8]) -> Option<&'a [u8]> {
    path.strip_prefix(dir)?.strip_prefix(&[SEP])
}

impl LiveRules {
    pub(super) fn compile(table: &Table, inputs: &GuardInputs<'_>) -> Result<Self, PolicyError> {
        let fold_case = table.fold_case;
        let abs = |text: &'static str| {
            AbsPath::parse(text.as_ref())
                .map(|p| fold(p.bytes(), fold_case))
                .map_err(|_| PolicyError::BadAnchor(text))
        };
        let home = &inputs.home;
        let partial = table.live.partial_downloads.iter().map(|&(rel, suffixes)| {
            let mut dir = home.as_path().to_path_buf();
            dir.extend(rel.split('/'));
            (
                fold(dir.as_os_str().as_encoded_bytes(), fold_case),
                suffixes,
            )
        });
        let scopes = table.live.user_cache_scopes.iter().map(|scope| {
            let comps = scope.split('/');
            comps.map(|c| Comp::new(c.as_bytes(), fold_case)).collect()
        });
        Ok(Self {
            fold_case,
            home: fold(home.bytes(), fold_case),
            scopes: scopes.collect(),
            powerlog: table.live.powerlog_stores.map(abs).transpose()?,
            edr: match table.live.edr {
                Some((root, vendors)) => Some((
                    abs(root)?,
                    vendors
                        .iter()
                        .map(|v| fold(v.as_bytes(), fold_case))
                        .collect(),
                )),
                None => None,
            },
            sqlite: table.live.sqlite_suffixes,
            partial: partial.collect(),
        })
    }

    /// PowerLog, EDR, user-cache scope owner and in-use family for `path`.
    pub(super) fn check<'p>(&self, path: &'p AbsPath, scratch: &mut Vec<u8>) -> LiveChecks<'p> {
        let raw = path.bytes();
        scratch.clear();
        scratch.extend_from_slice(raw);
        if self.fold_case {
            scratch.make_ascii_lowercase();
        }
        let folded = &scratch[..];
        let mut checks = LiveChecks::default();

        if let Some(stores) = &self.powerlog
            && let Some(rest) = below(folded, stores)
        {
            let mut parts = rest.split(|&b| b == SEP);
            checks.powerlog = matches!(
                (parts.next(), parts.next(), parts.next()),
                (Some(_), Some(file), None) if file.starts_with(b"current")
            );
        }
        if let Some((root, vendors)) = &self.edr
            && let Some(rest) = below(folded, root)
        {
            checks.edr = vendors.iter().any(|v| contains(rest, v));
        }
        if let Some(rest) = below(folded, &self.home) {
            let offset = raw.len() - rest.len();
            checks.scope_owner = self.scope_owner(rest, &raw[offset..]);
        }

        let name = folded.rsplit(|&b| b == SEP).next().unwrap_or(folded);
        let companion = COMPANIONS.iter().find(|c| name.ends_with(c.as_bytes()));
        let base = &name[..name.len() - companion.map_or(0, |c| c.len())];
        if self.sqlite.iter().any(|s| base.ends_with(s.as_bytes())) {
            match sqlite_family(path, companion.copied()) {
                Some(family) => checks.family = Some(family),
                None => checks.family_unreadable = true,
            }
        } else if self.partial.iter().any(|(dir, suffixes)| {
            below(folded, dir).is_some_and(|rest| {
                !rest.contains(&SEP) && suffixes.iter().any(|s| rest.ends_with(s.as_bytes()))
            })
        }) {
            checks.family = Some(vec![path.clone()]);
        }
        checks
    }

    /// The owner id when `rest` (home-relative, folded) lies in a user-cache scope. `raw` is the
    /// same span unfolded, so the id keeps its spelling.
    fn scope_owner<'p>(&self, rest: &[u8], raw: &'p [u8]) -> Option<&'p str> {
        self.scopes.iter().find_map(|scope| {
            let mut owner = None;
            let mut at = 0;
            let mut parts = rest.split(|&b| b == SEP);
            for comp in scope {
                let part = parts.next()?;
                match comp {
                    Comp::Lit(name) if **name == *part => {}
                    Comp::Glob(_) => owner = Some(&raw[at..at + part.len()]),
                    Comp::Lit(_) => return None,
                }
                at += part.len() + 1;
            }
            let owner = std::str::from_utf8(owner?).ok()?;
            // Reverse-DNS names only: `Homebrew` and other named trees have their own probes.
            (owner.contains('.') && !owner.starts_with('.')).then_some(owner)
        })
    }
}

/// The database and its companions, with the path as spelled first: a companion may be named
/// in another case. `None` when a companion's name is not UTF-8 and so cannot be split.
fn sqlite_family(path: &AbsPath, companion: Option<&str>) -> Option<Vec<AbsPath>> {
    let name = path.name()?;
    let base: OsString = match companion {
        None => name.to_os_string(),
        Some(companion) => {
            let name = name.to_str()?;
            name[..name.len() - companion.len()].into()
        }
    };
    let sibling = |suffix: &str| {
        let mut name = base.clone();
        name.push(suffix);
        AbsPath::trusted(path.as_path().with_file_name(name))
    };
    let mut family = vec![path.clone(), sibling("")];
    family.extend(COMPANIONS.iter().map(|c| sibling(c)));
    family.dedup();
    Some(family)
}
