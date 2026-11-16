//! The Check screen: type or paste a path and see whether dustpan keeps it. It asks the guard
//! through `dustpan::check`, the code `dustpan explain` uses, and only reads.
use super::facts::Environment;
use super::frame::{self, cut_end};
use super::rows::{self, Row, Tone};
use super::theme::Look;
use crate::check::{Checked, Checker};
use dustpan_core::guard::Verdict;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use std::ffi::OsStr;
use unicode_width::UnicodeWidthStr;

pub const PLACEHOLDER: &str = "Type a path, e.g. ~/Library/Caches";
pub const ELIGIBLE: &str = "dustpan may clean this when Clean arrives";
const MAX_INPUT: usize = 4096;
const EMPTY_HINT: &str = "Drag a file in, or paste a path, to check it at once.";
const UNMATCHED_QUOTE: &str =
    "This paste has an unmatched quote. Paste one path at a time, or type it.";
const AMBIGUOUS: &str = "This looks like several paths, or one path with spaces. Quote it, or paste one path at a time.";

/// The field holds one typed path, taken literally. A paste never goes into it: it is checked at
/// once, as its own submission.
#[derive(Clone, Debug)]
pub struct CheckView {
    pub input: String,
    /// Newest first.
    pub results: Vec<Checked>,
    pub error: Option<String>,
    pub scroll: usize,
}

impl CheckView {
    /// The guard is built again on every Enter. Opening only checks that it can be built, so a
    /// problem shows before the first path is typed.
    pub fn open(env: &Environment) -> Self {
        CheckView {
            error: Checker::open(&|name| env.get(name)).err(),
            input: String::new(),
            results: Vec::new(),
            scroll: 0,
        }
    }

    /// Typed text is taken as it is, the way `dustpan explain` takes its arguments.
    pub fn type_char(&mut self, c: char) {
        if !c.is_control() && self.input.len() < MAX_INPUT {
            self.input.push(c);
        }
    }

    /// A paste is its own submission. It never joins the field, and the typed text stays as it is.
    /// A paste that cannot be split without guessing checks nothing and says why.
    pub fn paste(&mut self, pasted: &str, env: &Environment) {
        let exists = |path: &str| {
            let full = match (path.strip_prefix('~'), env.get("HOME")) {
                (Some(rest), Some(home)) if rest.is_empty() || rest.starts_with('/') => {
                    format!("{home}{rest}")
                }
                _ => path.to_owned(),
            };
            std::fs::symlink_metadata(full).is_ok()
        };
        match pasted_paths(pasted, exists) {
            Ok(paths) if paths.is_empty() => {}
            Ok(paths) => {
                self.check(&paths, env);
            }
            Err(refused) => self.error = Some(refused.to_string()),
        }
    }

    pub fn erase(&mut self) {
        self.input.pop();
    }

    pub fn clear(&mut self) {
        self.input.clear();
    }

    /// Enter. Checks exactly what is in the field, as one path.
    pub fn submit(&mut self, env: &Environment) {
        if self.input.trim().is_empty() {
            self.error = Some("Type or paste a path first.".to_owned());
            return;
        }
        let typed = std::mem::take(&mut self.input);
        if !self.check(std::slice::from_ref(&typed), env) {
            self.input = typed;
        }
    }

    /// One check and one result for each path, from a guard built now so it reads the whitelist
    /// as it is now. The results go on top, in the order given. False when nothing was checked.
    fn check(&mut self, paths: &[String], env: &Environment) -> bool {
        let checker = match Checker::open(&|name| env.get(name)) {
            Ok(checker) => checker,
            Err(message) => {
                self.error = Some(message);
                return false;
            }
        };
        let judged: Result<Vec<Checked>, String> = checker
            .check_each(paths.iter().map(|path| OsStr::new(path.as_str())))
            .collect();
        match judged {
            Ok(checked) => {
                self.results.splice(0..0, checked);
                self.error = None;
                self.scroll = 0;
                true
            }
            Err(message) => {
                self.error = Some(message);
                false
            }
        }
    }
}

/// Why a paste checked nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refused {
    UnmatchedQuote,
    Ambiguous,
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Refused::UnmatchedQuote => UNMATCHED_QUOTE,
            Refused::Ambiguous => AMBIGUOUS,
        })
    }
}

/// The paths in a paste. Each line stands alone, and a blank line is ignored.
/// - A line with a quote or a backslash is a shell word list ([`shell_words`]), and each word is
///   a path. Terminal and iTerm2 drags, GNOME Terminal drags and shell copies all arrive so.
/// - Any other line with no whitespace is one path.
/// - Any other line with whitespace is one path when `exists` finds the whole line. Else it is
///   one path for each word when `exists` finds every word. Else it is refused, never guessed.
///
/// One refused line refuses the whole paste, so nothing is checked from a paste that was only
/// partly understood. A backslash is an escape everywhere but Windows, where it separates
/// folders.
pub fn pasted_paths(raw: &str, exists: impl Fn(&str) -> bool) -> Result<Vec<String>, Refused> {
    paths_in(raw, exists, !cfg!(windows))
}

fn paths_in(
    raw: &str,
    exists: impl Fn(&str) -> bool,
    backslash_escapes: bool,
) -> Result<Vec<String>, Refused> {
    let mut paths = Vec::new();
    for line in raw.split(['\n', '\r']).filter(|l| !l.trim().is_empty()) {
        let shell = line.contains(['\'', '"']) || (backslash_escapes && line.contains('\\'));
        if shell {
            paths.extend(shell_words(line, backslash_escapes)?);
            continue;
        }
        let line = line.trim();
        if !line.contains(char::is_whitespace) || exists(line) {
            paths.push(line.to_owned());
            continue;
        }
        let words: Vec<&str> = line.split_whitespace().collect();
        if !words.iter().all(|word| exists(word)) {
            return Err(Refused::Ambiguous);
        }
        paths.extend(words.into_iter().map(str::to_owned));
    }
    Ok(paths)
}

/// POSIX shell words. Single quotes are literal. Double quotes let a backslash escape only `\`,
/// `"`, `$` and a backtick. Outside quotes a backslash escapes the next character, and unquoted
/// whitespace ends a word. A quote opened and never closed is [`Refused::UnmatchedQuote`].
fn shell_words(line: &str, backslash_escapes: bool) -> Result<Vec<String>, Refused> {
    let mut words = Vec::new();
    let mut word: Option<String> = None;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            c if c.is_whitespace() => words.extend(word.take()),
            '\'' => {
                let text = word.get_or_insert_with(String::new);
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(inner) => text.push(inner),
                        None => return Err(Refused::UnmatchedQuote),
                    }
                }
            }
            '"' => {
                let text = word.get_or_insert_with(String::new);
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') if backslash_escapes => match chars.next() {
                            Some(e @ ('\\' | '"' | '$' | '`')) => text.push(e),
                            Some(other) => {
                                text.push('\\');
                                text.push(other);
                            }
                            None => return Err(Refused::UnmatchedQuote),
                        },
                        Some(inner) => text.push(inner),
                        None => return Err(Refused::UnmatchedQuote),
                    }
                }
            }
            '\\' if backslash_escapes => {
                let text = word.get_or_insert_with(String::new);
                text.push(chars.next().unwrap_or('\\'));
            }
            other => word.get_or_insert_with(String::new).push(other),
        }
    }
    words.extend(word);
    Ok(words)
}

fn verdict_rows(checked: &Checked, width: usize) -> Vec<Row> {
    let mut rows: Vec<Row> = frame::wrap_exact(&checked.path, width.saturating_sub(2))
        .into_iter()
        .map(|line| Row::of(Tone::Bold, format!(" {line}")))
        .collect();
    let (label, tone, reason) = match &checked.verdict {
        Verdict::Eligible => ("Eligible", Tone::Accent, ELIGIBLE.to_owned()),
        Verdict::Kept(reason) => ("Kept", Tone::Bold, reason.to_string()),
    };
    let lead = format!("   {label}  ");
    let room = width.saturating_sub(lead.width() + 1).max(10);
    for (i, line) in frame::wrap(&reason, room).into_iter().enumerate() {
        rows.push(if i == 0 {
            Row::runs(vec![(tone, lead.clone()), (Tone::Plain, line)])
        } else {
            Row::of(Tone::Plain, format!("{}{line}", " ".repeat(lead.width())))
        });
    }
    if let Some(physical) = &checked.physical {
        let lead = "   physical path: ";
        let room = width.saturating_sub(lead.width() + 1).max(10);
        for (i, line) in frame::wrap_exact(&physical.to_string(), room)
            .into_iter()
            .enumerate()
        {
            let pad = if i == 0 { lead } else { "                  " };
            rows.push(Row::of(Tone::Dim, format!("{pad}{line}")));
        }
    }
    rows
}

pub fn rows(view: &CheckView, width: usize) -> Vec<Row> {
    if view.results.is_empty() {
        let mut rows = vec![Row::of(Tone::Plain, " Results show here, newest first.")];
        rows.extend(
            frame::wrap(EMPTY_HINT, width.saturating_sub(2))
                .into_iter()
                .map(|line| Row::of(Tone::Dim, format!(" {line}"))),
        );
        return rows;
    }
    let mut rows = Vec::new();
    for (i, checked) in view.results.iter().enumerate() {
        if i > 0 {
            rows.push(Row::blank());
        }
        rows.extend(verdict_rows(checked, width));
    }
    rows
}

/// The rows under the field: the error, wrapped so its cause stays readable, or the promise.
fn note_rows(look: &Look, view: &CheckView, body: Rect) -> Vec<Row> {
    let Some(message) = &view.error else {
        return vec![Row::of(
            Tone::Dim,
            " Enter checks it. Nothing is changed or removed.",
        )];
    };
    let width = body.width as usize;
    let max_rows = usize::from(body.height.saturating_sub(4)).max(1);
    let mut lines = frame::wrap(message, width.saturating_sub(2));
    if lines.len() > max_rows {
        lines.truncate(max_rows);
        if let Some(last) = lines.last_mut() {
            *last = cut_end(
                &format!("{last}{}", look.glyphs.ellipsis),
                width.saturating_sub(2),
                look.glyphs.ellipsis,
            );
        }
    }
    lines
        .into_iter()
        .map(|line| Row::of(Tone::Accent, format!(" {line}")))
        .collect()
}

fn results_height(body: Rect, note_rows: usize) -> u16 {
    body.height
        .saturating_sub(2 + u16::try_from(note_rows).unwrap_or(u16::MAX))
}

pub fn max_scroll(look: &Look, view: &CheckView, body: Rect) -> usize {
    let width = body.width as usize;
    rows::max_scroll(
        rows(view, width).len(),
        results_height(body, note_rows(look, view, body).len()),
    )
}

pub fn render(frame: &mut Frame, look: &Look, view: &CheckView, body: Rect) {
    let width = body.width as usize;
    let note = note_rows(look, view, body);
    let note_height = u16::try_from(note.len()).unwrap_or(u16::MAX);
    let field = Rect::new(body.x, body.y, body.width, 1);
    let note_area = Rect::new(body.x, body.y + 1, body.width, note_height);
    let rule = Rect::new(body.x, body.y + 1 + note_height, body.width, 1);
    let results = Rect::new(
        body.x,
        body.y + (2 + note_height).min(body.height),
        body.width,
        results_height(body, note.len()),
    );
    frame.render_widget(Paragraph::new(field_line(look, &view.input, width)), field);
    rows::render(frame, look, &note, 0, note_area);
    frame.render_widget(Paragraph::new(look.glyphs.hline.repeat(width)), rule);
    rows::render(frame, look, &rows(view, width), view.scroll, results);
}

fn field_line<'a>(look: &Look, input: &str, width: usize) -> Line<'a> {
    let lead = " Path  ";
    let room = width.saturating_sub(lead.width() + 2);
    let mut spans = vec![Span::styled(lead, look.bold())];
    if input.is_empty() {
        spans.push(Span::styled(" ", look.selected()));
        spans.push(Span::styled(
            cut_end(PLACEHOLDER, room.saturating_sub(1), look.glyphs.ellipsis),
            look.dim(),
        ));
    } else {
        let mut tail: Vec<char> = Vec::new();
        let mut used = 0;
        for c in input.chars().rev() {
            let cell = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
            if used + cell > room.saturating_sub(1) {
                break;
            }
            used += cell;
            tail.push(c);
        }
        spans.push(Span::raw(tail.iter().rev().collect::<String>()));
        spans.push(Span::styled(" ", look.selected()));
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(raw: &str, existing: &[&str]) -> Result<Vec<String>, Refused> {
        paths_in(raw, |p| existing.contains(&p), true)
    }

    type Case<'a> = (
        &'a str,
        &'a str,
        &'a [&'a str],
        Result<Vec<String>, Refused>,
    );

    fn ok(paths: &[&str]) -> Result<Vec<String>, Refused> {
        Ok(paths.iter().map(|p| (*p).to_owned()).collect())
    }

    #[test]
    fn a_paste_is_split_by_the_shell_word_rules_and_never_guessed() {
        let cases: &[Case] = &[
            ("one plain path", "/a/b", &[], ok(&["/a/b"])),
            (
                "single-quoted pair",
                "'/a/x' '/a/y' ",
                &[],
                ok(&["/a/x", "/a/y"]),
            ),
            (
                "double-quoted pair",
                "\"/a/x\" \"/a/y\"",
                &[],
                ok(&["/a/x", "/a/y"]),
            ),
            (
                "escapes and a quote",
                r"/a/My\ Folder\ \(1\) '/a/keepme'",
                &[],
                ok(&["/a/My Folder (1)", "/a/keepme"]),
            ),
            (
                "a trailing escaped space is kept",
                r"/a/trail\  ",
                &[],
                ok(&["/a/trail "]),
            ),
            ("an escaped backslash", r"/a/b\\c", &[], ok(&[r"/a/b\c"])),
            (
                "single quotes keep backslashes",
                r"'/a/b\c'",
                &[],
                ok(&[r"/a/b\c"]),
            ),
            (
                "double quotes escape only four",
                r#""/a/\$x\"y\`z\\w\q""#,
                &[],
                ok(&[r#"/a/$x"y`z\w\q"#]),
            ),
            (
                "a quote joins the word it touches",
                r#"/a/"x y"z"#,
                &[],
                ok(&["/a/x yz"]),
            ),
            ("an empty quoted word is a word", "''", &[], ok(&[""])),
            (
                "a lone trailing backslash stays",
                "/a/b\\",
                &[],
                ok(&["/a/b\\"]),
            ),
            ("an escaped apostrophe", r"/a/it\'s", &[], ok(&["/a/it's"])),
            (
                "tilde words stay for the checker",
                "'~/a' ~/b",
                &[],
                ok(&["~/a", "~/b"]),
            ),
            (
                "unmatched single quote",
                "'/a/x",
                &[],
                Err(Refused::UnmatchedQuote),
            ),
            (
                "unmatched double quote",
                "/a/x \"/a/y",
                &[],
                Err(Refused::UnmatchedQuote),
            ),
            (
                "a quote that never closes after an escape",
                r#""/a/x\"#,
                &[],
                Err(Refused::UnmatchedQuote),
            ),
            (
                "one refused line refuses the paste",
                "/a/x\n'/a/y",
                &[],
                Err(Refused::UnmatchedQuote),
            ),
            (
                "an existing path with a space, unquoted",
                "/a/my dir",
                &["/a/my dir"],
                ok(&["/a/my dir"]),
            ),
            (
                "the whole line wins over its words",
                "/a/b /c",
                &["/a/b /c", "/a/b", "/c"],
                ok(&["/a/b /c"]),
            ),
            (
                "a missing path with a space",
                "/a/Backup ~2024",
                &[],
                Err(Refused::Ambiguous),
            ),
            (
                "one existing word is not enough",
                "/a/one /a/missing",
                &["/a/one"],
                Err(Refused::Ambiguous),
            ),
            (
                "two existing paths split by a space",
                "/a/one /a/two",
                &["/a/one", "/a/two"],
                ok(&["/a/one", "/a/two"]),
            ),
            (
                "the folder `a ` with a missing child",
                "/h/a /c",
                &["/h/a "],
                Err(Refused::Ambiguous),
            ),
            (
                "tilde words that exist",
                "~/a ~/b",
                &["~/a", "~/b"],
                ok(&["~/a", "~/b"]),
            ),
            (
                "lines are separate, blanks ignored",
                "/a/one\r\n\n  \n/a/two\n",
                &[],
                ok(&["/a/one", "/a/two"]),
            ),
            (
                "each line is judged on its own",
                "/a/one two\n/a/three",
                &["/a/one two"],
                ok(&["/a/one two", "/a/three"]),
            ),
            ("empty", "", &[], ok(&[])),
            ("whitespace only", " \t\r\n \n", &[], ok(&[])),
        ];
        for (name, raw, existing, want) in cases {
            assert_eq!(&parse(raw, existing), want, "{name}: {raw:?}");
        }
    }

    #[test]
    fn on_windows_a_backslash_is_a_folder_separator_and_quotes_still_work() {
        let paths = |raw| paths_in(raw, |_| false, false);
        assert_eq!(paths(r"C:\Users\me\x"), ok(&[r"C:\Users\me\x"]));
        assert_eq!(paths(r#""C:\a b" 'D:\c'"#), ok(&[r"C:\a b", r"D:\c"]));
    }

    #[test]
    fn a_refusal_says_what_to_do_next() {
        assert_eq!(
            Refused::UnmatchedQuote.to_string(),
            "This paste has an unmatched quote. Paste one path at a time, or type it."
        );
        assert_eq!(
            Refused::Ambiguous.to_string(),
            "This looks like several paths, or one path with spaces. Quote it, or paste one path at a time."
        );
    }

    fn tempdir() -> tempfile::TempDir {
        tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap()
    }

    fn fixture_env(home: &std::path::Path) -> Environment {
        let home = home.to_string_lossy().into_owned();
        Environment::fixed(&[
            ("HOME", home.clone()),
            ("USERPROFILE", home.clone()),
            ("APPDATA", format!("{home}/AppData/Roaming")),
            ("LOCALAPPDATA", format!("{home}/AppData/Local")),
        ])
    }

    #[cfg(unix)]
    fn lines(view: &CheckView) -> Vec<String> {
        rows(view, 100).iter().map(Row::text).collect()
    }

    #[cfg(unix)]
    #[test]
    fn a_plain_folder_in_the_home_is_eligible_in_plain_words() {
        let dir = tempdir();
        let env = fixture_env(dir.path());
        let folder = dir.path().join("scratch");
        std::fs::create_dir_all(&folder).unwrap();
        let mut view = CheckView::open(&env);
        view.paste(&folder.to_string_lossy(), &env);
        let lines = lines(&view);
        let eligible = lines.iter().position(|l| l.contains("Eligible")).unwrap();
        let shown: String = lines[..eligible].iter().map(|l| &l[1..]).collect();
        assert_eq!(shown, folder.to_string_lossy(), "{lines:?}");
        assert_eq!(lines[eligible], format!("   Eligible  {ELIGIBLE}"));
        assert!(view.input.is_empty() && view.error.is_none());
    }

    #[cfg(unix)]
    #[test]
    fn a_protected_path_is_kept_with_the_reason_explain_prints() {
        let dir = tempdir();
        let env = fixture_env(dir.path());
        let mut view = CheckView::open(&env);
        view.paste("/usr", &env);
        let lines = lines(&view);
        assert!(lines[1].starts_with("   Kept  "), "{lines:?}");
        let checker = Checker::open(&|name| env.get(name)).unwrap();
        let expected = checker
            .check_each([OsStr::new("/usr")])
            .next()
            .unwrap()
            .unwrap()
            .verdict
            .kept()
            .unwrap()
            .to_string();
        assert!(lines[1].contains(&expected) || lines[2].contains(&expected));
    }

    #[cfg(unix)]
    #[test]
    fn a_tilde_path_is_checked_against_the_home_folder() {
        let dir = tempdir();
        let env = fixture_env(dir.path());
        let mut view = CheckView::open(&env);
        view.paste("~/Library/Caches", &env);
        let shown = &lines(&view)[0];
        assert!(shown.ends_with("Library/Caches"), "{shown}");
        assert!(!shown.contains('~'), "{shown}");
    }

    #[cfg(unix)]
    #[test]
    fn results_stack_newest_first() {
        let dir = tempdir();
        let env = fixture_env(dir.path());
        let mut view = CheckView::open(&env);
        for name in ["first", "second"] {
            view.paste(&dir.path().join(name).to_string_lossy(), &env);
        }
        let paths: Vec<_> = view.results.iter().map(|r| r.path.clone()).collect();
        assert!(
            paths[0].ends_with("second") && paths[1].ends_with("first"),
            "{paths:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_paste_of_two_paths_is_checked_as_two_paths() {
        let dir = tempdir();
        let env = fixture_env(dir.path());
        let (scratch, keepme) = (dir.path().join("scratch"), dir.path().join("keepme"));
        std::fs::create_dir_all(&scratch).unwrap();
        std::fs::create_dir_all(&keepme).unwrap();
        for raw in [
            format!("{}\n{}", scratch.display(), keepme.display()),
            format!("{} {} ", scratch.display(), keepme.display()),
        ] {
            let mut view = CheckView::open(&env);
            view.paste(&raw, &env);
            let paths: Vec<_> = view.results.iter().map(|r| r.path.clone()).collect();
            assert_eq!(
                paths,
                [
                    scratch.to_string_lossy().into_owned(),
                    keepme.to_string_lossy().into_owned()
                ],
                "{raw:?}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_typed_backslash_is_part_of_the_path() {
        let dir = tempdir();
        let env = fixture_env(dir.path());
        let mut view = CheckView::open(&env);
        let typed = format!("{}/back\\slash", dir.path().display());
        typed.chars().for_each(|c| view.type_char(c));
        view.submit(&env);
        assert_eq!(view.results[0].path, typed);
    }

    #[test]
    fn an_empty_submit_asks_for_a_path_and_a_missing_home_keeps_the_error_on_screen() {
        let dir = tempdir();
        let env = fixture_env(dir.path());
        let mut view = CheckView::open(&env);
        view.submit(&env);
        assert_eq!(view.error.as_deref(), Some("Type or paste a path first."));
        let none = Environment::fixed(&[]);
        let mut broken = CheckView::open(&none);
        assert!(
            broken
                .error
                .as_deref()
                .unwrap()
                .contains("home folder is not set")
        );
        "/x".chars().for_each(|c| broken.type_char(c));
        broken.submit(&none);
        assert!(broken.error.is_some() && broken.results.is_empty());
        assert_eq!(
            broken.input, "/x",
            "the typed path stays so the user can retry"
        );
    }
}
