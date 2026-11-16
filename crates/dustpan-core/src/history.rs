//! `dustpan history`: sessions and per-item records read back from the operations log.
//!
//! Read-only (SAFE-37): this module takes the log's bytes and returns data, and the binary
//! opens the two files for reading only. The fields are Mole's HIST-02 contract
//! (`mac-optimize-core-cli.md` porting note 9): sessions with `command`, `run_id`,
//! `attribution`, `started_at`, `ended_at`, `items`, `size`, `operation_count`, `failed_tasks`
//! and `actions`, and deletions with `timestamp`, `mode`, `status`, `size_kb` and `path`.
//!
//! Mole keeps a second file, `deletions.log`, for the per-item list. Dustpan writes one row per
//! item, once, and derives the deletions from the item rows of `operations.log`, so the two
//! lists cannot disagree.
use crate::oplog::{self, Parsed};
use std::collections::HashMap;

/// `--limit N`: 1 to 200, default 20 (HIST-03).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Limit(u8);

impl Limit {
    pub const DEFAULT: Self = Self(20);
    pub const MAX: u8 = 200;

    pub fn new(n: u64) -> Option<Self> {
        u8::try_from(n)
            .ok()
            .filter(|n| (1..=Self::MAX).contains(n))
            .map(Self)
    }

    pub fn get(self) -> usize {
        usize::from(self.0)
    }
}

/// How a session's rows were tied together. Every dustpan row carries a run id, so `Run` is
/// the norm; `Command` is a row without one, which only a hand-edited or foreign log has.
/// Mole's third value, `ambiguous`, exists for its legacy logs and is never produced here.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Attribution {
    Run,
    Command,
}

impl Attribution {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Run => "run",
            Self::Command => "command",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Actions {
    pub removed: u64,
    pub trashed: u64,
    pub skipped: u64,
    pub failed: u64,
    pub rebuilt: u64,
    pub other: u64,
}

/// One session. Text fields are decoded with U+FFFD for invalid UTF-8; a text renderer still
/// escapes control bytes with `path::escaped`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SessionSummary {
    /// `clean`, or `clean-dry-run` for a preview.
    pub command: String,
    pub run_id: String,
    pub attribution: Attribution,
    /// `YYYY-MM-DD HH:MM:SS`, local time.
    pub started_at: String,
    /// Empty when the session has no end marker: it crashed or is still running.
    pub ended_at: String,
    pub items: u64,
    pub size: String,
    pub operation_count: u64,
    pub failed_tasks: u64,
    pub actions: Actions,
    /// `+HHMM` from the start marker, empty for a session whose start Mole wrote or rotated away.
    offset: String,
}

/// One item row: what happened to one candidate.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DeletionRecord {
    /// `YYYY-MM-DDTHH:MM:SS+HHMM`, as Mole's `deletions.log` writes it. No offset when the
    /// session's start marker is not in the log.
    pub timestamp: String,
    /// `permanent` for the Delete sink, `trash` for the Trash sink (Mole's two modes).
    pub mode: &'static str,
    /// `ok` for `REMOVED` and `TRASHED`; the reason code for `SKIPPED` and `FAILED`.
    pub status: String,
    /// Kibibytes, rounded up; `None` (`null`) when the row has no known size.
    pub size_kb: Option<u64>,
    pub path: String,
}

/// Everything both log files hold, oldest first.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct History {
    sessions: Vec<SessionSummary>,
    deletions: Vec<DeletionRecord>,
    /// Sessions with no end marker yet, by command and run id.
    open: HashMap<(String, String), usize>,
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Splits ` +HHMM` off a start stamp. Mole's markers have none.
fn split_offset(at: &[u8]) -> (&[u8], &[u8]) {
    match at.len().checked_sub(6).map(|i| at.split_at(i)) {
        Some((stamp, tail))
            if tail[0] == b' '
                && matches!(tail[1], b'+' | b'-')
                && tail[2..].iter().all(u8::is_ascii_digit) =>
        {
            (stamp, &tail[1..])
        }
        _ => (at, &[]),
    }
}

impl History {
    /// Reads `operations.log.old`, then `operations.log`. Never fails: a line that does not
    /// parse is skipped, a row with no start marker (rotated away, or a torn start) opens its
    /// session, and a session with no end marker is reported not ended.
    pub fn read(old: &[u8], current: &[u8]) -> Self {
        let mut history = Self::default();
        let lines = old
            .split(|&b| b == b'\n')
            .chain(current.split(|&b| b == b'\n'));
        for line in lines {
            if let Some(parsed) = oplog::parse(line) {
                history.add(parsed);
            }
        }
        history
    }

    /// The open session for a command field, opening one when a row or an end marker comes
    /// first (its start was rotated away or torn).
    fn session(&mut self, command: &[u8], run: Option<&[u8]>, at: &[u8]) -> usize {
        let key = (text(command), run.map(text).unwrap_or_default());
        match self.open.get(&key) {
            Some(&index) => index,
            None => self.start(command, run, at),
        }
    }

    fn start(&mut self, command: &[u8], run: Option<&[u8]>, at: &[u8]) -> usize {
        let key = (text(command), run.map(text).unwrap_or_default());
        let (at, offset) = split_offset(at);
        self.sessions.push(SessionSummary {
            command: key.0.clone(),
            run_id: key.1.clone(),
            attribution: if run.is_some() {
                Attribution::Run
            } else {
                Attribution::Command
            },
            started_at: text(at),
            ended_at: String::new(),
            items: 0,
            size: String::new(),
            operation_count: 0,
            failed_tasks: 0,
            actions: Actions::default(),
            offset: text(offset),
        });
        let index = self.sessions.len() - 1;
        self.open.insert(key, index);
        index
    }

    fn add(&mut self, line: Parsed<'_>) {
        match line {
            Parsed::Start { command, run, at } => {
                self.start(command, run, at);
            }
            Parsed::End {
                command,
                run,
                at,
                items,
                size,
            } => {
                let index = self.session(command, run, at);
                self.open
                    .remove(&(text(command), run.map(text).unwrap_or_default()));
                let session = &mut self.sessions[index];
                session.ended_at = text(at);
                if let Some(items) = items {
                    session.items = items;
                }
                session.size = text(size);
            }
            Parsed::Item {
                at,
                command,
                run,
                action,
                path,
                detail,
            } => {
                let index = self.session(command, run, at);
                let session = &mut self.sessions[index];
                session.operation_count += 1;
                let actions = &mut session.actions;
                match action {
                    b"REMOVED" => actions.removed += 1,
                    b"TRASHED" => actions.trashed += 1,
                    b"SKIPPED" => actions.skipped += 1,
                    b"FAILED" => actions.failed += 1,
                    b"REBUILT" => actions.rebuilt += 1,
                    b"TASK_FAILED" => session.failed_tasks += 1,
                    _ => actions.other += 1,
                }
                // Only a sink writes `sink=`, so a row Mole wrote is a session count, not an
                // item record: Mole's own item records live in its `deletions.log`.
                let Some(sink) = detail.sink else { return };
                let status = match (action, detail.reason) {
                    (b"REMOVED" | b"TRASHED", _) => "ok".to_owned(),
                    (_, Some(reason)) => text(reason),
                    (other, None) => text(other).to_ascii_lowercase(),
                };
                let size_kb = detail
                    .size
                    .and_then(|size| std::str::from_utf8(size).ok())
                    .and_then(|size| size.trim_end_matches('+').parse::<u64>().ok())
                    .map(|bytes| bytes.div_ceil(1024));
                let timestamp = text(at).replacen(' ', "T", 1) + &session.offset;
                self.deletions.push(DeletionRecord {
                    timestamp,
                    mode: if sink == b"trash" {
                        "trash"
                    } else {
                        "permanent"
                    },
                    status,
                    size_kb,
                    path: text(path),
                });
            }
        }
    }

    /// The newest `limit` sessions, newest first.
    pub fn sessions(&self, limit: Limit) -> impl Iterator<Item = &SessionSummary> {
        self.sessions.iter().rev().take(limit.get())
    }

    /// The newest `limit` item records, newest first.
    pub fn deletions(&self, limit: Limit) -> impl Iterator<Item = &DeletionRecord> {
        self.deletions.iter().rev().take(limit.get())
    }
}
