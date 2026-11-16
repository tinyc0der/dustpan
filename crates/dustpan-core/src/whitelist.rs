pub(crate) mod glob;

use crate::config::Read;
use crate::guard::Homes;
use crate::path::{AbsPath, ShapeError, escaped};
use crate::paths::Os;
use glob::Glob;
pub use glob::GlobError;
use std::ffi::OsStr;
use std::fmt;
use std::path::Path;

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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Origin {
    Default,
    Safety,
    User,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Pattern {
    /// An absolute path whose names may hold `*`, `?` and `[...]`. It protects what it names,
    /// everything inside it, and every folder above it (removing the folder removes the entry).
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
        why: EntryError,
    },
}

/// Why one line cannot be applied.
#[derive(Debug)]
pub enum EntryError {
    Shape(ShapeError),
    Glob(GlobError),
    /// `~/C:\x` on Windows: the home's name is followed by a whole path of its own.
    OutsideHome,
}

impl fmt::Display for EntryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Shape(why) => why.fmt(f),
            Self::Glob(why) => why.fmt(f),
            Self::OutsideHome => f.write_str("it starts at the home folder but names another"),
        }
    }
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

fn under_home(os: Os, raw: &str) -> Option<&str> {
    let rest = ["~", "$HOME", "${HOME}"]
        .iter()
        .find_map(|home| raw.strip_prefix(home))?;
    let separator = |c: char| c == '/' || (os == Os::Windows && c == '\\');
    (rest.is_empty() || rest.starts_with(separator)).then(|| rest.trim_start_matches(separator))
}

/// One pattern per home for an entry under `~`, so `HOME` pointed elsewhere still leaves the
/// account's home kept (I-64). One pattern for any other entry.
fn expand(os: Os, homes: &Homes, raw: &str) -> Result<Vec<Pattern>, EntryError> {
    if raw == FINDER_METADATA {
        return Ok(vec![Pattern::FinderMetadata]);
    }
    let Some(rest) = under_home(os, raw) else {
        return Ok(vec![parse(raw.as_ref())?]);
    };
    homes
        .iter()
        .map(|home| {
            let joined = home.as_path().join(rest);
            if !joined.starts_with(home.as_path()) {
                return Err(EntryError::OutsideHome);
            }
            parse(&joined)
        })
        .collect()
}

fn parse(expanded: &Path) -> Result<Pattern, EntryError> {
    let path = AbsPath::parse(expanded.as_os_str()).map_err(EntryError::Shape)?;
    for name in path.names() {
        for fold_case in [false, true] {
            Glob::parse(name, fold_case).map_err(EntryError::Glob)?;
        }
    }
    Ok(Pattern::Path(path))
}

fn built_in(os: Os, homes: &Homes, rows: &[&str], origin: Origin, into: &mut Vec<Entry>) {
    for raw in rows {
        let patterns = expand(os, homes, raw).expect("a built-in entry is an absolute path");
        into.extend(
            patterns
                .into_iter()
                .map(|pattern| Entry { pattern, origin }),
        );
    }
}

impl Whitelist {
    /// What applies while no user file exists: the defaults and the hard-safety entries.
    pub fn defaults(os: Os, homes: &Homes) -> Self {
        let mut entries = Vec::new();
        built_in(os, homes, SAFETY, Origin::Safety, &mut entries);
        built_in(os, homes, DEFAULTS, Origin::Default, &mut entries);
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
/// A line loses the ASCII whitespace at either end, as Mole's `[:space:]` under `LC_ALL=C`
/// trims it (so CRLF endings and trailing spaces do not matter). A space outside ASCII, such as
/// U+2002, stays part of the name. Blank lines and lines that start with `#` are skipped. A line that cannot name a place stops the load: a
/// relative path, one with `..`, an unknown class, a backward range, or a bracket element of
/// more than one character.
pub fn load(os: Os, homes: &Homes, file: &AbsPath, read: Read) -> Result<Whitelist, LoadError> {
    let text = match read {
        Read::Missing => return Ok(Whitelist::defaults(os, homes)),
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
        let line = line.trim_matches(|c: char| c.is_ascii_whitespace() || c == '\x0b');
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match expand(os, homes, line) {
            Ok(patterns) => entries.extend(patterns.into_iter().map(|pattern| Entry {
                pattern,
                origin: Origin::User,
            })),
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
    built_in(os, homes, SAFETY, Origin::Safety, &mut entries);
    Ok(Whitelist {
        file: Some(file.clone()),
        entries,
    })
}
