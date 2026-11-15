//! The whitelist: paths the user (and dustpan itself) never wants cleaned. Layer 5 of the guard.
//!
//! The binary reads the file and hands core its text as a [`Read`]; this module parses it and
//! merges the built-in entries. An unreadable file is an error, never an empty list, so a run
//! that cannot learn what the user protects does not start (SAFE-11, Mole's WL-01).
use crate::config::Read;
use crate::path::{AbsPath, ShapeError, escaped};
use crate::paths::Os;
use std::ffi::OsStr;
use std::fmt;

/// Mole's `FINDER_METADATA` sentinel, as written in a file.
const FINDER_METADATA: &str = "FINDER_METADATA";

/// Mole's `DEFAULT_WHITELIST_PATTERNS` (`lib/core/base.sh:172-189`). Apply while no user file
/// exists. A user file replaces them, as it does in Mole.
const DEFAULTS: &[&str] = &[
    "~/Library/Caches/ms-playwright*",
    "~/Library/Caches/camoufox*",
    "~/Library/Caches/Cypress*",
    "~/.gradle/caches/*",
    "~/.gradle/daemon/*",
    "~/.ollama/models/*",
    "~/Library/Caches/com.nssurge.surge-mac/*",
    "~/Library/Application Support/com.nssurge.surge-mac/*",
    "~/Library/Caches/org.R-project.R/R/renv/*",
    "~/Library/Caches/JetBrains*",
    "~/Library/Caches/com.jetbrains.toolbox*",
    "~/Library/Caches/tealdeer/tldr-pages",
    "~/Library/Application Support/JetBrains*",
    "~/Library/Caches/com.apple.finder",
    "~/Library/Mobile Documents*",
    FINDER_METADATA,
];

/// Mole's `SAFETY_WHITELIST_PATTERNS` (`lib/core/base.sh:194-233`). Merged into every whitelist,
/// so no user file can remove one.
const SAFETY: &[&str] = &[
    FINDER_METADATA,
    "~/Library/Caches/com.apple.FontRegistry*",
    "~/Library/Caches/com.apple.spotlight*",
    "~/Library/Caches/com.apple.Spotlight*",
    "~/Library/Caches/CloudKit*",
    "~/Library/Caches/kitty",
    "~/Library/Caches/pypoetry/virtualenvs*",
    "~/Library/Caches/org.R-project.R/R/renv/*",
    "~/Library/Caches/pnpm/dlx*",
    "~/Library/Caches/llama.cpp",
    "~/Library/Caches/Buzz",
];

/// Where an entry came from. Decides what `explain` says and whether a user file can drop it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Origin {
    Default,
    Safety,
    User,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Pattern {
    /// An absolute path whose components may hold `*`. It protects what it names, everything
    /// inside it, and every folder above it (removing the folder removes the entry).
    Path(AbsPath),
    /// Every file named `.DS_Store`.
    FinderMetadata,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Entry {
    pub pattern: Pattern,
    pub origin: Origin,
}

/// Every entry that applies, merged and in the order they are asked: user, safety, default.
#[derive(Clone, Debug)]
pub struct Whitelist {
    file: Option<AbsPath>,
    entries: Vec<Entry>,
}

#[derive(Debug)]
pub enum LoadError {
    Unreadable {
        file: AbsPath,
        why: String,
    },
    BadEntry {
        file: AbsPath,
        line: usize,
        entry: String,
        why: ShapeError,
    },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreadable { file, why } => write!(
                f,
                "cannot read the whitelist {file}: {why}; nothing is cleaned until it can be read"
            ),
            Self::BadEntry {
                file,
                line,
                entry,
                why,
            } => write!(
                f,
                "{file} line {line}: {why}; fix or remove the entry {entry}"
            ),
        }
    }
}

impl std::error::Error for LoadError {}

/// An entry as written, with `~` and `~/` standing for the home folder.
fn expand(os: Os, home: &AbsPath, raw: &str) -> Result<Pattern, ShapeError> {
    if raw == FINDER_METADATA {
        return Ok(Pattern::FinderMetadata);
    }
    let under_home = raw.strip_prefix('~').and_then(|rest| match rest {
        "" => Some(""),
        _ => rest.strip_prefix('/').or_else(|| {
            (os == Os::Windows)
                .then(|| rest.strip_prefix('\\'))
                .flatten()
        }),
    });
    let expanded = match under_home {
        Some(rest) => home.as_path().join(rest),
        None => raw.into(),
    };
    AbsPath::parse(expanded.as_os_str()).map(Pattern::Path)
}

fn built_in(os: Os, home: &AbsPath, rows: &[&str], origin: Origin, into: &mut Vec<Entry>) {
    for raw in rows {
        let pattern = expand(os, home, raw).expect("a built-in entry is an absolute path");
        into.push(Entry { pattern, origin });
    }
}

impl Whitelist {
    /// What applies while no user file exists: the defaults and the hard-safety entries.
    pub fn defaults(os: Os, home: &AbsPath) -> Self {
        let mut entries = Vec::new();
        built_in(os, home, SAFETY, Origin::Safety, &mut entries);
        built_in(os, home, DEFAULTS, Origin::Default, &mut entries);
        Self {
            file: None,
            entries,
        }
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// The user file the `User` entries came from, `None` when there was none.
    pub fn file(&self) -> Option<&AbsPath> {
        self.file.as_ref()
    }
}

/// Parses the whitelist `file` as the binary read it.
///
/// A line is trimmed (so CRLF endings and trailing spaces do not matter), and blank lines and
/// lines that start with `#` are skipped. A line that cannot name a place, such as a relative
/// path or one with `..`, stops the load: skipping it would protect less than the user wrote.
pub fn load(os: Os, home: &AbsPath, file: &AbsPath, read: Read) -> Result<Whitelist, LoadError> {
    let text = match read {
        Read::Missing => return Ok(Whitelist::defaults(os, home)),
        Read::Unreadable { why } => {
            return Err(LoadError::Unreadable {
                file: file.clone(),
                why,
            });
        }
        Read::Text(text) => text,
    };
    let mut entries = Vec::new();
    for (index, line) in text.trim_start_matches('\u{feff}').lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match expand(os, home, line) {
            Ok(pattern) => entries.push(Entry {
                pattern,
                origin: Origin::User,
            }),
            Err(why) => {
                return Err(LoadError::BadEntry {
                    file: file.clone(),
                    line: index + 1,
                    entry: escaped(OsStr::new(line)).to_string(),
                    why,
                });
            }
        }
    }
    built_in(os, home, SAFETY, Origin::Safety, &mut entries);
    Ok(Whitelist {
        file: Some(file.clone()),
        entries,
    })
}
