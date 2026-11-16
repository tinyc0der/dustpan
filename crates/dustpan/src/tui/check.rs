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

pub const PLACEHOLDER: &str = "Type or paste a path, e.g. ~/Library/Caches";
pub const ELIGIBLE: &str = "dustpan may clean this when Clean arrives";
const MAX_INPUT: usize = 4096;
const EMPTY_HINT: &str = "Try a folder like ~/Library/Caches, or drag one in from Finder.";

/// The field holds the paths literally. A newline between two of them comes only from a paste
/// of several paths, because a typed key never makes one.
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

    /// Pasted text may hold Terminal's escapes, quotes, or several paths. See [`pasted_paths`].
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
        for (i, path) in pasted_paths(pasted, exists).iter().enumerate() {
            if i > 0 && self.input.len() < MAX_INPUT {
                self.input.push('\n');
            }
            path.chars().for_each(|c| self.type_char(c));
        }
    }

    pub fn erase(&mut self) {
        self.input.pop();
    }

    pub fn clear(&mut self) {
        self.input.clear();
    }

    /// Enter. One check and one result for each path in the field, from a guard built now so it
    /// reads the whitelist as it is now.
    pub fn submit(&mut self, env: &Environment) {
        let paths: Vec<&str> = self
            .input
            .split('\n')
            .filter(|path| !path.trim().is_empty())
            .collect();
        if paths.is_empty() {
            self.error = Some("Type or paste a path first.".to_owned());
            return;
        }
        let checker = match Checker::open(&|name| env.get(name)) {
            Ok(checker) => checker,
            Err(message) => {
                self.error = Some(message);
                return;
            }
        };
        let judged: Result<Vec<Checked>, String> = checker
            .check_each(paths.iter().map(|path| OsStr::new(*path)))
            .collect();
        match judged {
            Ok(checked) => {
                self.results.splice(0..0, checked);
                self.input.clear();
                self.error = None;
                self.scroll = 0;
            }
            Err(message) => self.error = Some(message),
        }
    }
}

fn unescape(raw: &str) -> Vec<(char, bool)> {
    let mut chars = Vec::new();
    let mut rest = raw.chars();
    while let Some(c) = rest.next() {
        match (c, cfg!(windows)) {
            ('\\', false) => match rest.next() {
                Some(escaped) => chars.push((escaped, true)),
                None => chars.push((c, false)),
            },
            _ => chars.push((c, false)),
        }
    }
    chars
}

fn blank(&(c, escaped): &(char, bool)) -> bool {
    !escaped && c.is_whitespace()
}

fn text(chars: &[(char, bool)]) -> String {
    chars.iter().map(|&(c, _)| c).collect()
}

/// The paths a terminal pasted. Each line is its own path. Terminal.app escapes a dragged
/// file's name with backslashes (`My\ Folder`), puts a plain space between several dragged
/// files, and a copied path may sit in quotes or carry a trailing space. A line that is not
/// itself an existing path, and splits on its unescaped spaces into pieces that all start with
/// `/` or `~`, is several paths. Any other line is one path, so a plain `/Users/x/My Folder`
/// stays whole. Paths are never glued together.
pub fn pasted_paths(raw: &str, exists: impl Fn(&str) -> bool) -> Vec<String> {
    raw.split(['\n', '\r'])
        .flat_map(|line| line_paths(line, &exists))
        .collect()
}

fn line_paths(line: &str, exists: &impl Fn(&str) -> bool) -> Vec<String> {
    let trimmed = line.trim();
    for quote in ['\'', '"'] {
        if let Some(inner) = trimmed
            .strip_prefix(quote)
            .and_then(|rest| rest.strip_suffix(quote))
        {
            return vec![inner.to_owned()];
        }
    }
    let chars = unescape(line);
    let start = chars.iter().position(|c| !blank(c)).unwrap_or(chars.len());
    let end = chars
        .iter()
        .rposition(|c| !blank(c))
        .map_or(start, |i| i + 1);
    let chars = &chars[start..end];
    let whole = text(chars);
    if whole.is_empty() {
        return Vec::new();
    }
    if exists(&whole) {
        return vec![whole];
    }
    let pieces: Vec<String> = chars
        .split(blank)
        .filter(|piece| !piece.is_empty())
        .map(text)
        .collect();
    if pieces.len() > 1 && pieces.iter().all(|p| p.starts_with(['/', '~'])) {
        pieces
    } else {
        vec![whole]
    }
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
        let divider = format!(" {} ", look.glyphs.bullet);
        let shown = input.replace('\n', &divider);
        let mut tail: Vec<char> = Vec::new();
        let mut used = 0;
        for c in shown.chars().rev() {
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

    fn never(_: &str) -> bool {
        false
    }

    fn pasted(raw: &str) -> Vec<String> {
        pasted_paths(raw, never)
    }

    #[test]
    fn a_dragged_in_path_loses_its_backslashes_and_trailing_space() {
        if cfg!(windows) {
            return;
        }
        assert_eq!(
            pasted(r"/Users/me/My\ Folder\ \(1\)/  "),
            ["/Users/me/My Folder (1)/"]
        );
        assert_eq!(pasted(r"/a/b\\c"), [r"/a/b\c"]);
        assert_eq!(pasted(r"/a/trailing\ "), ["/a/trailing "]);
        assert_eq!(pasted("/a/b\\"), ["/a/b\\"]);
    }

    #[test]
    fn quotes_around_a_path_are_removed_and_spaces_inside_stay() {
        assert_eq!(pasted("'/Users/me/My Folder'"), ["/Users/me/My Folder"]);
        assert_eq!(pasted("  \"/Users/me/x y\"  "), ["/Users/me/x y"]);
        assert_eq!(pasted("'/it's'"), ["/it's"]);
    }

    #[test]
    fn a_plain_or_tilde_path_is_kept_and_blank_input_is_empty() {
        assert_eq!(pasted("~/Library/Caches"), ["~/Library/Caches"]);
        assert!(pasted("   ").is_empty());
        assert!(pasted("").is_empty());
        assert_eq!(pasted("'"), ["'"]);
    }

    #[test]
    fn lines_are_separate_paths() {
        assert_eq!(pasted("/a/one\n/a/two"), ["/a/one", "/a/two"]);
        assert_eq!(pasted("/a/one\r\n/a/two\r\n"), ["/a/one", "/a/two"]);
        assert_eq!(pasted("/a/one\n\n  \n~/two"), ["/a/one", "~/two"]);
    }

    #[test]
    fn terminals_two_file_drag_is_two_paths() {
        if cfg!(windows) {
            return;
        }
        assert_eq!(
            pasted(r"/a/My\ Folder /b/keepme "),
            ["/a/My Folder", "/b/keepme"]
        );
        assert_eq!(pasted("~/one /b/two"), ["~/one", "/b/two"]);
    }

    #[test]
    fn a_path_with_a_plain_space_stays_one_path() {
        assert_eq!(pasted("/Users/x/My Folder"), ["/Users/x/My Folder"]);
        assert_eq!(pasted("/Users/x/a /b c"), ["/Users/x/a /b c"]);
    }

    #[test]
    fn a_line_that_is_an_existing_path_is_not_split() {
        let exists = |p: &str| p == "/a/b /c";
        assert_eq!(pasted_paths("/a/b /c", exists), ["/a/b /c"]);
        assert_eq!(pasted_paths("/a/b /d", exists), ["/a/b", "/d"]);
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
        view.submit(&env);
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
        view.submit(&env);
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
        view.submit(&env);
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
            view.submit(&env);
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
            view.submit(&env);
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
        broken.paste("/x", &none);
        broken.submit(&none);
        assert!(broken.error.is_some() && broken.results.is_empty());
        assert_eq!(
            broken.input, "/x",
            "the typed path stays so the user can retry"
        );
    }
}
