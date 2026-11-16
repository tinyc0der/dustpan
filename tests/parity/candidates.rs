use crate::error::ParseError;
use std::path::Path;

pub const SAVED_HOME: &str = "/FIXTURE/home";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub path: String,
    pub size: String,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Parsed {
    pub inside: Vec<Candidate>,
    pub outside: Vec<String>,
    pub escaped: Vec<String>,
    pub nested: Vec<String>,
}

pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\x1b' => match chars.next() {
                Some('[') => {
                    for n in chars.by_ref() {
                        if n.is_ascii_alphabetic() {
                            break;
                        }
                    }
                }
                Some(']') => {
                    for n in chars.by_ref() {
                        if n == '\x07' {
                            break;
                        }
                    }
                }
                _ => {}
            },
            '\r' => {}
            c => out.push(c),
        }
    }
    out
}

pub fn home_relative(path: &str, home: &Path) -> Option<String> {
    match place(path, home) {
        Placement::Inside(p) => Some(p),
        Placement::Escape | Placement::Host => None,
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Placement {
    Inside(String),
    /// Under the fixture root but outside the fixture home: a trap or a redirect.
    Escape,
    /// Not under the fixture at all: the host machine's own state.
    Host,
}

pub fn place(path: &str, home: &Path) -> Placement {
    if path == "~" || path.starts_with("~/") {
        return Placement::Inside(mask_generated_name(path.to_string()));
    }
    let under = |base: &Path| {
        let base = base.to_string_lossy();
        let base = base.trim_end_matches('/');
        path.strip_prefix(base)
            .filter(|rest| rest.is_empty() || rest.starts_with('/'))
            .map(str::to_string)
    };
    if let Some(rest) = under(home) {
        let relative = if rest.is_empty() {
            "~".into()
        } else {
            format!("~{rest}")
        };
        return Placement::Inside(mask_generated_name(relative));
    }
    match home.parent().and_then(under) {
        Some(_) => Placement::Escape,
        None => Placement::Host,
    }
}

impl Parsed {
    fn push(&mut self, path: &str, size: &str, nested: bool, home: &Path) {
        match place(path, home) {
            Placement::Inside(p) if nested => self.nested.push(p),
            Placement::Inside(p) => self.inside.push(Candidate {
                path: p,
                size: size.into(),
            }),
            Placement::Escape => self.escaped.push(path.into()),
            Placement::Host => self.outside.push(path.into()),
        }
    }
}

fn mask_generated_name(path: String) -> String {
    match path.strip_prefix("~/.npm/_logs/") {
        Some(name) if name.ends_with("-debug-0.log") => {
            "~/.npm/_logs/<timestamp>-debug-0.log".into()
        }
        _ => path,
    }
}

fn fail(source: &'static str, line_no: usize, line: &str, expected: &'static str) -> ParseError {
    ParseError {
        source,
        line_no,
        line: line.to_string(),
        expected,
    }
}

fn is_dry_run_banner(line: &str) -> bool {
    ["→", "○"].iter().any(|icon| {
        line.strip_prefix(icon)
            .is_some_and(|rest| rest.trim_start().starts_with("DRY RUN MODE"))
    })
}

fn is_size(s: &str) -> bool {
    let digits = s.trim_end_matches(|c: char| c.is_ascii_alphabetic());
    let unit = &s[digits.len()..];
    !digits.is_empty()
        && digits.chars().all(|c| c.is_ascii_digit() || c == '.')
        && matches!(unit, "B" | "KB" | "MB" | "GB" | "TB")
}

pub fn parse_clean_list(text: &str, home: &Path) -> Result<Parsed, ParseError> {
    const SRC: &str = "clean-list.txt";
    let mut parsed = Parsed::default();
    for (i, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.len() > 6 && line.starts_with("=== ") && line.ends_with(" ===") {
            continue;
        }
        let Some((path, detail)) = line.split_once("  # ") else {
            return Err(fail(
                SRC,
                i + 1,
                raw,
                "`<path>  # <size>`, a `=== Section ===` header, or a `#` comment",
            ));
        };
        let (size, nested) = match detail.split_once(", counted under ") {
            Some((size, under)) => (size, Some(under)),
            None => (detail, None),
        };
        if !is_size(size.trim()) {
            return Err(fail(SRC, i + 1, raw, "a size such as `1.0MB` after `# `"));
        }
        parsed.push(path.trim(), size.trim(), nested.is_some(), home);
    }
    Ok(parsed)
}

const CLOUD_PREFIX: &str = "[cloud] ";

const PURGE_NOISE: &[&str] = &[
    "Purge Project Artifacts",
    "Dry run complete - no changes made",
    "No old project artifacts to clean",
    "No artifacts were removed.",
    "Some artifacts were skipped or could not be processed.",
];

pub fn parse_purge_stdout(raw: &str, home: &Path) -> Result<Parsed, ParseError> {
    const SRC: &str = "mo purge stdout";
    let mut parsed = Parsed::default();
    for (i, l) in strip_ansi(raw).lines().enumerate() {
        let line = l.trim();
        if line.is_empty()
            || PURGE_NOISE.contains(&line)
            || is_dry_run_banner(line)
            || line.starts_with("Would free approximately:")
            || line.starts_with("Free space:")
            || (line.len() > 10 && line.chars().all(|c| c == '='))
        {
            continue;
        }
        let Some(row) = line.strip_prefix("✓ [DRY RUN] ") else {
            return Err(fail(
                SRC,
                i + 1,
                l,
                "a `✓ [DRY RUN] <path>, <size>` row or a known banner line",
            ));
        };
        let Some((path, size)) = row.rsplit_once(", ") else {
            return Err(fail(SRC, i + 1, l, "`<path>, <size>` after `[DRY RUN]`"));
        };
        if !is_size(size.trim()) {
            return Err(fail(
                SRC,
                i + 1,
                l,
                "a size such as `2.1MB` after the last comma",
            ));
        }
        let path = path.trim();
        let path = path.strip_prefix(CLOUD_PREFIX).unwrap_or(path);
        parsed.push(path, size.trim(), false, home);
    }
    Ok(parsed)
}

const FOUND_INSTALLER: &str = "[DEBUG] Found installer:";

/// Mole's `--debug` stderr names every installer it found by full path
/// (`log.sh` `debug_file_action`). The menu on stdout shows one page of
/// truncated names with a source label instead, so it is not parsed.
pub fn parse_installer_debug(stderr: &str, home: &Path) -> Result<Parsed, ParseError> {
    const SRC: &str = "mo installer stderr";
    let mut parsed = Parsed::default();
    for (i, l) in strip_ansi(stderr).lines().enumerate() {
        let Some(rest) = l.trim().strip_prefix(FOUND_INSTALLER) else {
            continue;
        };
        let Some(path) = rest.trim_start().strip_prefix("* ") else {
            return Err(fail(
                SRC,
                i + 1,
                l,
                "`[DEBUG] Found installer:   * <full path>`",
            ));
        };
        parsed.push(path, "", false, home);
    }
    Ok(parsed)
}
