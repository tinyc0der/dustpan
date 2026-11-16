//! The History screen: past sessions and the deletion audit, read once when the screen opens.
//! It reads through `history_log`, the code `dustpan history` uses, and only reads.
use super::facts::Environment;
use super::frame::{cut_end, cut_middle, pad_end, pad_start};
use super::rows::{self, Row, Tone};
use super::theme::Look;
use crate::history_log::{self, Loaded};
use dustpan_core::history::{DeletionRecord, Limit, SessionSummary};
use dustpan_core::path::escaped;
use dustpan_core::size::Size;
use ratatui::Frame;
use ratatui::layout::Rect;
use std::ffi::OsStr;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Debug)]
pub struct HistoryView {
    pub loaded: Result<Loaded, String>,
    pub scroll: usize,
}

impl HistoryView {
    pub fn open(env: &Environment) -> Self {
        HistoryView {
            loaded: history_log::load(&|name| env.get(name)),
            scroll: 0,
        }
    }
}

const WHEN: usize = 16;

/// A reason code such as `owner-running` gets its full width once the window has room.
fn status_width(width: usize) -> usize {
    if width >= 100 { 16 } else { 7 }
}

fn safe(text: &str) -> String {
    escaped(OsStr::new(text)).to_string()
}

/// `2026-11-16 09:45`, from either log stamp (`... 09:45:00` or `...T09:45:00+0700`).
fn when(stamp: &str) -> String {
    let spaced: String = stamp
        .chars()
        .enumerate()
        .map(|(i, c)| if i == 10 && c == 'T' { ' ' } else { c })
        .take(WHEN)
        .collect();
    safe(&spaced)
}

fn session_status(session: &SessionSummary) -> String {
    let failed = session.actions.failed.max(session.failed_tasks);
    if session.ended_at.is_empty() {
        "not ended".to_owned()
    } else if failed > 0 {
        format!("{failed} failed")
    } else {
        "done".to_owned()
    }
}

fn session_row(look: &Look, session: &SessionSummary) -> Row {
    let ellipsis = look.glyphs.ellipsis;
    Row::of(
        Tone::Plain,
        format!(
            " {}  {}  {}  {}  {}",
            pad_end(&when(&session.started_at), WHEN),
            pad_end(&cut_end(&safe(&session.command), 10, ellipsis), 10),
            pad_start(&session.items.to_string(), 5),
            pad_start(&cut_end(&safe(&session.size), 8, ellipsis), 8),
            session_status(session),
        ),
    )
}

fn deletion_row(look: &Look, record: &DeletionRecord, width: usize) -> Row {
    let ellipsis = look.glyphs.ellipsis;
    let status = status_width(width);
    let size = record.size_kb.map_or_else(
        || "unknown".to_owned(),
        |kb| Size::Known(kb.saturating_mul(1024)).to_string(),
    );
    let lead = format!(
        " {}  {}  {}  {}  ",
        pad_end(&when(&record.timestamp), WHEN),
        pad_end(record.mode, 9),
        pad_end(&cut_end(&safe(&record.status), status, ellipsis), status),
        pad_start(&cut_end(&size, 8, ellipsis), 8),
    );
    let room = width.saturating_sub(lead.width() + 1).max(6);
    Row::of(
        Tone::Plain,
        format!("{lead}{}", cut_middle(&safe(&record.path), room, ellipsis)),
    )
}

pub fn rows(look: &Look, view: &HistoryView, width: usize) -> Vec<Row> {
    let loaded = match &view.loaded {
        Ok(loaded) => loaded,
        Err(message) => {
            return vec![
                Row::of(Tone::Bold, " The history could not be read."),
                Row::blank(),
                Row::of(Tone::Plain, format!(" {message}")),
            ]
            .into_iter()
            .flat_map(|row| wrap_row(row, width))
            .collect();
        }
    };
    let history = &loaded.history;
    let limit = Limit::DEFAULT;
    let sessions: Vec<_> = history.sessions(limit).collect();
    let deletions: Vec<_> = history.deletions(limit).collect();
    let mut rows = Vec::new();
    if sessions.is_empty() && deletions.is_empty() {
        rows.push(Row::of(Tone::Bold, " Nothing has been cleaned yet."));
        rows.push(Row::of(
            Tone::Plain,
            " Every removal dustpan makes will show up here.",
        ));
    } else {
        rows.push(Row::of(Tone::Bold, " Recent sessions"));
        rows.push(Row::of(
            Tone::Dim,
            format!(
                " {}  {}  {}  {}  {}",
                pad_end("Started", WHEN),
                pad_end("Command", 10),
                pad_start("Items", 5),
                pad_start("Size", 8),
                "Status"
            ),
        ));
        rows.extend(sessions.iter().map(|s| session_row(look, s)));
        rows.push(Row::blank());
        rows.push(Row::of(Tone::Bold, " Deletion audit"));
        rows.push(Row::of(
            Tone::Dim,
            format!(
                " {}  {}  {}  {}  Path",
                pad_end("When", WHEN),
                pad_end("Mode", 9),
                pad_end("Result", status_width(width)),
                pad_start("Size", 8),
            ),
        ));
        rows.extend(deletions.iter().map(|d| deletion_row(look, d, width)));
    }
    rows.push(Row::blank());
    let log = format!(" Log: {}", safe(&loaded.log));
    rows.push(Row::of(
        Tone::Dim,
        cut_middle(&log, width.saturating_sub(1), look.glyphs.ellipsis),
    ));
    rows
}

fn wrap_row(row: Row, width: usize) -> Vec<Row> {
    let text = row.text();
    if text.trim().is_empty() {
        return vec![row];
    }
    super::frame::wrap(&text, width.saturating_sub(2))
        .into_iter()
        .map(|line| Row::of(Tone::Plain, format!(" {line}")))
        .collect()
}

pub fn max_scroll(look: &Look, view: &HistoryView, width: usize, height: u16) -> usize {
    rows::max_scroll(rows(look, view, width).len(), height)
}

pub fn render(frame: &mut Frame, look: &Look, view: &HistoryView, body: Rect) {
    let rows = rows(look, view, body.width as usize);
    rows::render(frame, look, &rows, view.scroll, body);
}

#[cfg(test)]
mod tests {
    use super::*;
    use dustpan_core::paths::{self, Os};

    const LOG: &[u8] = b"\n# ========== clean run=20261116094500-42-9f3a session started at 2026-11-16 09:45:00 +0700 ==========\n\
[2026-11-16 09:45:01] [clean run=20261116094500-42-9f3a] REMOVED /home/a/.cache/x (sink=delete size=1500000)\n\
[2026-11-16 09:45:01] [clean run=20261116094500-42-9f3a] SKIPPED /home/a/.cache/y (sink=trash reason=owner-running)\n\
# ========== clean run=20261116094500-42-9f3a session ended at 2026-11-16 09:45:02, 1 items, 1.5 MB ==========\n";

    fn home_env(home: &std::path::Path) -> Environment {
        let home = home.to_string_lossy().into_owned();
        Environment::fixed(&[
            ("HOME", home.clone()),
            ("USERPROFILE", home.clone()),
            ("LOCALAPPDATA", format!("{home}/AppData/Local")),
        ])
    }

    fn log_dir(env: &Environment) -> std::path::PathBuf {
        paths::log_dir(Os::current(), &|name| env.get(name)).unwrap()
    }

    fn fixture(log: Option<&[u8]>) -> (tempfile::TempDir, HistoryView) {
        let home = tempfile::tempdir().unwrap();
        let env = home_env(home.path());
        if let Some(log) = log {
            let dir = log_dir(&env);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("operations.log"), log).unwrap();
        }
        let view = HistoryView::open(&env);
        (home, view)
    }

    fn text(view: &HistoryView, width: usize) -> Vec<String> {
        rows(&Look::new(false, true), view, width)
            .iter()
            .map(Row::text)
            .collect()
    }

    #[test]
    fn a_log_shows_its_session_and_its_audit_row() {
        let (_home, view) = fixture(Some(LOG));
        let lines = text(&view, 100);
        let session = lines.iter().find(|l| l.contains("clean ")).unwrap();
        assert!(session.contains("2026-11-16 09:45"), "{session}");
        assert!(
            session.contains("1.5 MB") && session.contains("done"),
            "{session}"
        );
        let removed = lines
            .iter()
            .find(|l| l.contains("/home/a/.cache/x"))
            .unwrap();
        assert!(
            removed.contains("permanent") && removed.contains("1.5 MB"),
            "{removed}"
        );
        assert!(lines.iter().any(|l| l.contains("owner-running")));
    }

    #[test]
    fn a_missing_log_says_nothing_has_been_cleaned_and_creates_nothing() {
        let (home, view) = fixture(None);
        let lines = text(&view, 60);
        assert_eq!(lines[0], " Nothing has been cleaned yet.");
        assert!(lines[1].contains("Every removal dustpan makes will show up here"));
        let env = home_env(home.path());
        assert!(
            !log_dir(&env).exists(),
            "opening History made the log folder"
        );
    }

    #[test]
    fn a_log_that_cannot_be_read_shows_the_cli_error_and_stays_a_screen() {
        let home = tempfile::tempdir().unwrap();
        let env = home_env(home.path());
        let dir = log_dir(&env).join("operations.log");
        std::fs::create_dir_all(dir).unwrap();
        let view = HistoryView::open(&env);
        let lines = text(&view, 80);
        assert_eq!(lines[0], " The history could not be read.");
        assert!(lines.iter().any(|l| l.contains("cannot read")), "{lines:?}");
    }

    #[test]
    fn an_unset_home_is_an_error_not_a_crash() {
        let view = HistoryView::open(&Environment::fixed(&[]));
        let lines = text(&view, 80);
        assert_eq!(lines[0], " The history could not be read.");
    }

    #[test]
    fn a_control_byte_in_the_log_cannot_move_the_cursor() {
        let log =
            b"\n# ========== clean run=r1 session started at 2026-11-16 09:45:00 ==========\n\
[2026-11-16 09:45:01] [clean run=r1] REMOVED /a/\x1b[2Jb (sink=delete size=10)\n";
        let (_home, view) = fixture(Some(log));
        assert!(text(&view, 100).iter().all(|l| !l.contains('\x1b')));
    }

    #[test]
    fn every_row_fits_the_width_it_was_built_for() {
        let (_home, view) = fixture(Some(LOG));
        for width in [60, 80, 120] {
            for line in text(&view, width) {
                assert!(
                    unicode_width::UnicodeWidthStr::width(line.as_str()) <= width,
                    "{width}: {line}"
                );
            }
        }
    }
}
