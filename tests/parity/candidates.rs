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
    let home = home.to_string_lossy();
    let relative = if path == "~" || path.starts_with("~/") {
        path.to_string()
    } else {
        match path.strip_prefix(home.as_ref())? {
            "" => "~".into(),
            r if r.starts_with('/') => format!("~{r}"),
            _ => return None,
        }
    };
    Some(mask_generated_name(relative))
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
        match (home_relative(path.trim(), home), nested) {
            (Some(p), Some(_)) => parsed.nested.push(p),
            (Some(p), None) => parsed.inside.push(Candidate {
                path: p,
                size: size.trim().into(),
            }),
            (None, _) => parsed.outside.push(path.trim().into()),
        }
    }
    Ok(parsed)
}

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
        match home_relative(path.trim(), home) {
            Some(p) => parsed.inside.push(Candidate {
                path: p,
                size: size.trim().into(),
            }),
            None => parsed.outside.push(path.trim().into()),
        }
    }
    Ok(parsed)
}

fn installer_source_dir(label: &str) -> Option<&'static str> {
    Some(match label {
        "Downloads" => "~/Downloads",
        "Desktop" => "~/Desktop",
        "Documents" => "~/Documents",
        "Public" => "~/Public",
        "Library" => "~/Library/Downloads",
        "Homebrew" => "~/Library/Caches/Homebrew",
        _ => return None,
    })
}

pub fn parse_installer_stdout(raw: &str) -> Result<Parsed, ParseError> {
    const SRC: &str = "mo installer stdout";
    let mut parsed = Parsed::default();
    for (i, l) in strip_ansi(raw).lines().enumerate() {
        let line = l.trim();
        if line.is_empty()
            || is_dry_run_banner(line)
            || line.starts_with("Select Installers to Remove")
            || line.starts_with("↑↓")
            || line == "No installer files to clean"
        {
            continue;
        }
        let row = line.strip_prefix('➤').unwrap_or(line).trim_start();
        let Some(row) = row.strip_prefix('○').or_else(|| row.strip_prefix('●')) else {
            return Err(fail(
                SRC,
                i + 1,
                l,
                "a menu row `○ <name> <size> | <source>` or a known banner line",
            ));
        };
        let Some((left, source)) = row.rsplit_once(" | ") else {
            return Err(fail(SRC, i + 1, l, "`<name> <size> | <source>`"));
        };
        let left = left.trim_end();
        let Some((name, size)) = left.rsplit_once(' ') else {
            return Err(fail(SRC, i + 1, l, "`<name> <size>` before the `|`"));
        };
        if !is_size(size) {
            return Err(fail(SRC, i + 1, l, "a size such as `1.0MB` before the `|`"));
        }
        let Some(dir) = installer_source_dir(source.trim()) else {
            return Err(fail(
                SRC,
                i + 1,
                l,
                "a known source label (Downloads, Desktop, Documents, Public, Library, Homebrew)",
            ));
        };
        parsed.inside.push(Candidate {
            path: format!("{dir}/{}", name.trim()),
            size: size.into(),
        });
    }
    Ok(parsed)
}
