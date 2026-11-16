//! The Check screen: type or paste a path and see whether dustpan keeps it. It asks the guard
//! through `dustpan::check`, the code `dustpan explain` uses, and only reads.
use super::facts::Environment;
use super::frame::{self, cut_end, cut_middle};
use super::rows::{self, Row, Tone};
use super::theme::Look;
use crate::check::{Checked, Checker};
use dustpan_core::guard::Verdict;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use std::ffi::OsStr;
use std::fmt;
use std::rc::Rc;
use unicode_width::UnicodeWidthStr;

pub const PLACEHOLDER: &str = "Type or paste a path, e.g. ~/Library/Caches";
pub const ELIGIBLE: &str = "dustpan may clean this when Clean arrives";
const FIELD_ROWS: u16 = 3;
const MAX_INPUT: usize = 4096;

#[derive(Clone)]
pub struct CheckView {
    guard: Result<Rc<Checker>, String>,
    pub input: String,
    /// Newest first.
    pub results: Vec<Checked>,
    pub error: Option<String>,
    pub scroll: usize,
}

impl fmt::Debug for CheckView {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CheckView")
            .field("guard", &self.guard.as_ref().map(|_| "built"))
            .field("input", &self.input)
            .field("results", &self.results.len())
            .field("error", &self.error)
            .finish()
    }
}

fn build(env: &Environment) -> Result<Rc<Checker>, String> {
    Checker::open(&|name| env.get(name)).map(Rc::new)
}

impl CheckView {
    /// Builds the guard once, here. A guard that cannot be built keeps its error on screen.
    pub fn open(env: &Environment) -> Self {
        let guard = build(env);
        CheckView {
            error: guard.as_ref().err().cloned(),
            guard,
            input: String::new(),
            results: Vec::new(),
            scroll: 0,
        }
    }

    pub fn type_char(&mut self, c: char) {
        if !c.is_control() && self.input.len() < MAX_INPUT {
            self.input.push(c);
        }
    }

    pub fn paste(&mut self, pasted: &str) {
        for c in pasted.chars() {
            self.type_char(c);
        }
    }

    pub fn erase(&mut self) {
        self.input.pop();
    }

    pub fn clear(&mut self) {
        self.input.clear();
    }

    /// Enter. Each check runs a fresh pass. A guard that failed to build is tried again, so
    /// fixing the whitelist file and pressing Enter works.
    pub fn submit(&mut self, env: &Environment) {
        let path = parse_typed(&self.input);
        if path.is_empty() {
            self.error = Some("Type or paste a path first.".to_owned());
            return;
        }
        if self.guard.is_err() {
            self.guard = build(env);
        }
        let checker = match &self.guard {
            Ok(checker) => checker,
            Err(message) => {
                self.error = Some(message.clone());
                return;
            }
        };
        match checker.check(OsStr::new(&path)) {
            Ok(checked) => {
                self.results.insert(0, checked);
                self.input.clear();
                self.error = None;
                self.scroll = 0;
            }
            Err(message) => self.error = Some(message),
        }
    }
}

/// The path the user meant from what a terminal handed over. Terminal.app escapes a dragged
/// file's name with backslashes (`My\ Folder`), and a copied one may sit in quotes or carry a
/// trailing space.
pub fn parse_typed(raw: &str) -> String {
    let trimmed = raw.trim();
    for quote in ['\'', '"'] {
        if let Some(inner) = trimmed
            .strip_prefix(quote)
            .and_then(|rest| rest.strip_suffix(quote))
        {
            return inner.to_owned();
        }
    }
    let mut chars: Vec<(char, bool)> = Vec::new();
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
    let blank = |&(c, escaped): &(char, bool)| !escaped && c.is_whitespace();
    let start = chars.iter().position(|c| !blank(c)).unwrap_or(chars.len());
    let end = chars
        .iter()
        .rposition(|c| !blank(c))
        .map_or(start, |i| i + 1);
    chars[start..end].iter().map(|&(c, _)| c).collect()
}

fn verdict_rows(look: &Look, checked: &Checked, width: usize) -> Vec<Row> {
    let ellipsis = look.glyphs.ellipsis;
    let mut rows = vec![Row::of(
        Tone::Bold,
        format!(
            " {}",
            cut_middle(&checked.path, width.saturating_sub(2), ellipsis)
        ),
    )];
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
        rows.push(Row::of(
            Tone::Dim,
            format!(
                "   physical path: {}",
                cut_middle(
                    &physical.to_string(),
                    room.saturating_sub(15).max(10),
                    ellipsis
                )
            ),
        ));
    }
    rows
}

pub fn rows(look: &Look, view: &CheckView, width: usize) -> Vec<Row> {
    if view.results.is_empty() {
        return vec![
            Row::of(Tone::Plain, " Results show here, newest first."),
            Row::of(
                Tone::Dim,
                " Try a folder like ~/Library/Caches, or drag one in from Finder.",
            ),
        ];
    }
    let mut rows = Vec::new();
    for (i, checked) in view.results.iter().enumerate() {
        if i > 0 {
            rows.push(Row::blank());
        }
        rows.extend(verdict_rows(look, checked, width));
    }
    rows
}

fn results_height(body: Rect) -> u16 {
    body.height.saturating_sub(FIELD_ROWS)
}

pub fn max_scroll(look: &Look, view: &CheckView, body: Rect) -> usize {
    rows::max_scroll(
        rows(look, view, body.width as usize).len(),
        results_height(body),
    )
}

pub fn render(frame: &mut Frame, look: &Look, view: &CheckView, body: Rect) {
    let width = body.width as usize;
    let field = Rect::new(body.x, body.y, body.width, 1);
    let note = Rect::new(body.x, body.y + 1, body.width, 1);
    let rule = Rect::new(body.x, body.y + 2, body.width, 1);
    let results = Rect::new(
        body.x,
        body.y + FIELD_ROWS.min(body.height),
        body.width,
        results_height(body),
    );
    frame.render_widget(Paragraph::new(field_line(look, &view.input, width)), field);
    let note_line = match &view.error {
        Some(message) => Line::styled(
            format!(
                " {}",
                cut_end(message, width.saturating_sub(2), look.glyphs.ellipsis)
            ),
            look.accent(),
        ),
        None => Line::styled(
            " Enter checks it. Nothing is changed or removed.",
            look.dim(),
        ),
    };
    frame.render_widget(Paragraph::new(note_line), note);
    frame.render_widget(Paragraph::new(look.glyphs.hline.repeat(width)), rule);
    let rows = rows(look, view, width);
    rows::render(frame, look, &rows, view.scroll, results);
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
        let tail: String = {
            let mut shown: Vec<char> = Vec::new();
            let mut used = 0;
            for c in input.chars().rev() {
                let cell = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
                if used + cell > room.saturating_sub(1) {
                    break;
                }
                used += cell;
                shown.push(c);
            }
            shown.iter().rev().collect()
        };
        spans.push(Span::raw(tail));
        spans.push(Span::styled(" ", look.selected()));
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dragged_in_path_loses_its_backslashes_and_trailing_space() {
        if cfg!(windows) {
            return;
        }
        assert_eq!(
            parse_typed(r"/Users/me/My\ Folder\ \(1\)/  "),
            "/Users/me/My Folder (1)/"
        );
        assert_eq!(parse_typed(r"/a/b\\c"), r"/a/b\c");
        assert_eq!(parse_typed(r"/a/trailing\ "), "/a/trailing ");
        assert_eq!(parse_typed("/a/b\\"), "/a/b\\");
    }

    #[test]
    fn quotes_around_a_path_are_removed_and_spaces_inside_stay() {
        assert_eq!(parse_typed("'/Users/me/My Folder'"), "/Users/me/My Folder");
        assert_eq!(parse_typed("  \"/Users/me/x y\"  "), "/Users/me/x y");
        assert_eq!(parse_typed("'/it's'"), "/it's");
    }

    #[test]
    fn a_plain_or_tilde_path_is_kept_and_blank_input_is_empty() {
        assert_eq!(parse_typed("~/Library/Caches"), "~/Library/Caches");
        assert_eq!(parse_typed("   "), "");
        assert_eq!(parse_typed(""), "");
        assert_eq!(parse_typed("'"), "'");
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

    fn lines(view: &CheckView) -> Vec<String> {
        rows(&Look::new(false, true), view, 100)
            .iter()
            .map(Row::text)
            .collect()
    }

    #[test]
    fn a_plain_folder_in_the_home_is_eligible_in_plain_words() {
        let dir = tempdir();
        let env = fixture_env(dir.path());
        let folder = dir.path().join("scratch");
        std::fs::create_dir_all(&folder).unwrap();
        let mut view = CheckView::open(&env);
        view.paste(&folder.to_string_lossy());
        view.submit(&env);
        let lines = lines(&view);
        assert!(
            lines[0].starts_with(" ") && lines[0].ends_with("scratch"),
            "{lines:?}"
        );
        assert_eq!(lines[1], format!("   Eligible  {ELIGIBLE}"));
        assert!(view.input.is_empty() && view.error.is_none());
    }

    #[test]
    fn a_protected_path_is_kept_with_the_reason_explain_prints() {
        let dir = tempdir();
        let env = fixture_env(dir.path());
        let mut view = CheckView::open(&env);
        let protected = if cfg!(windows) {
            r"C:\Windows"
        } else {
            "/System"
        };
        view.paste(protected);
        view.submit(&env);
        let lines = lines(&view);
        assert!(lines[1].starts_with("   Kept  "), "{lines:?}");
        let expected = Checker::open(&|name| env.get(name))
            .unwrap()
            .check(OsStr::new(protected))
            .unwrap()
            .verdict
            .kept()
            .unwrap()
            .to_string();
        assert!(lines[1].contains(&expected) || lines[2].contains(&expected));
    }

    #[test]
    fn a_tilde_path_is_checked_against_the_home_folder() {
        let dir = tempdir();
        let env = fixture_env(dir.path());
        let mut view = CheckView::open(&env);
        view.paste("~/Library/Caches");
        view.submit(&env);
        let shown = &lines(&view)[0];
        assert!(
            shown.ends_with("Library/Caches") || shown.ends_with(r"Library\Caches"),
            "{shown}"
        );
        assert!(!shown.contains('~'), "{shown}");
    }

    #[test]
    fn results_stack_newest_first() {
        let dir = tempdir();
        let env = fixture_env(dir.path());
        let mut view = CheckView::open(&env);
        for name in ["first", "second"] {
            view.paste(&dir.path().join(name).to_string_lossy());
            view.submit(&env);
        }
        let paths: Vec<_> = view.results.iter().map(|r| r.path.clone()).collect();
        assert!(
            paths[0].ends_with("second") && paths[1].ends_with("first"),
            "{paths:?}"
        );
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
        broken.paste("/x");
        broken.submit(&none);
        assert!(broken.error.is_some() && broken.results.is_empty());
        assert_eq!(
            broken.input, "/x",
            "the typed path stays so the user can retry"
        );
    }
}
