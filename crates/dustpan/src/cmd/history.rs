//! `dustpan history [--json] [--limit N]`: past sessions and per-item records from the
//! operations log. Read-only (SAFE-37): it opens `operations.log.old` and `operations.log` for
//! reading and nothing else, so running it never creates, rotates or changes a log file. Exit 0,
//! with an empty history when there is no log yet; 1 when a log exists and cannot be read; 2 for
//! a usage error (`--limit 0` and `--limit 201` are refused while the arguments are parsed).
use crate::exit::Exit;
use dustpan_core::history::{DeletionRecord, History, Limit, SessionSummary};
use dustpan_core::path::escaped;
use dustpan_core::paths::{self, Os};
use dustpan_core::size::Size;
use dustpan_sys::FileStore;
use std::ffi::OsStr;
use std::io::{self, Write};
use std::path::Path;

fn read(dir: &Path, name: &str) -> io::Result<Vec<u8>> {
    match std::fs::read(dir.join(name)) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        other => other,
    }
}

pub fn run(json: bool, limit: Limit, out: &mut dyn Write) -> Exit {
    let env = |key: &str| std::env::var(key).ok();
    let Some(dir) = paths::log_dir(Os::current(), &env) else {
        let _ = writeln!(
            io::stderr(),
            "dustpan: the home folder is not set or not an absolute path, so there is no log folder to read"
        );
        return Exit::Failed;
    };
    let logs = read(&dir, FileStore::OLD).and_then(|old| Ok((old, read(&dir, FileStore::FILE)?)));
    let (old, current) = match logs {
        Ok(logs) => logs,
        Err(e) => {
            let _ = writeln!(io::stderr(), "dustpan: cannot read the operations log: {e}");
            return Exit::Failed;
        }
    };
    let history = History::read(&old, &current);
    let log = dir.join(FileStore::FILE).to_string_lossy().into_owned();
    // A closed pipe is not a failure of the command.
    let _ = if json {
        write_json(&history, limit, &log, out)
    } else {
        write_text(&history, limit, &log, out)
    };
    Exit::Done
}

fn quoted(text: &str) -> String {
    serde_json::Value::from(text).to_string()
}

fn session_json(s: &SessionSummary) -> String {
    let a = &s.actions;
    format!(
        r#"{{"command":{},"run_id":{},"attribution":{},"started_at":{},"ended_at":{},"items":{},"size":{},"operation_count":{},"failed_tasks":{},"actions":{{"removed":{},"trashed":{},"skipped":{},"failed":{},"rebuilt":{},"other":{}}}}}"#,
        quoted(&s.command),
        quoted(&s.run_id),
        quoted(s.attribution.code()),
        quoted(&s.started_at),
        quoted(&s.ended_at),
        s.items,
        quoted(&s.size),
        s.operation_count,
        s.failed_tasks,
        a.removed,
        a.trashed,
        a.skipped,
        a.failed,
        a.rebuilt,
        a.other
    )
}

fn deletion_json(d: &DeletionRecord) -> String {
    format!(
        r#"{{"timestamp":{},"mode":{},"status":{},"size_kb":{},"path":{}}}"#,
        quoted(&d.timestamp),
        quoted(d.mode),
        quoted(&d.status),
        d.size_kb
            .map_or_else(|| "null".to_owned(), |kb| kb.to_string()),
        quoted(&d.path)
    )
}

/// Mole's HIST-02 object with Mole's keys (porting note 9), plus `schema_version` and `command`
/// from PLAN's envelope, which Mole's scripts ignore. `logs.deletions` names the operations log
/// too: dustpan derives the item list from it and keeps no `deletions.log`.
fn write_json(history: &History, limit: Limit, log: &str, out: &mut dyn Write) -> io::Result<()> {
    let sessions: Vec<String> = history.sessions(limit).map(session_json).collect();
    let deletions: Vec<String> = history.deletions(limit).map(deletion_json).collect();
    writeln!(
        out,
        r#"{{"schema_version":1,"command":"history","logs":{{"operations":{0},"deletions":{0}}},"limit":{1},"sessions":[{2}],"deletions":[{3}]}}"#,
        quoted(log),
        limit.get(),
        sessions.join(","),
        deletions.join(",")
    )
}

/// Mole's text layout: recent sessions, the item audit, then the logs. Every field read from the
/// log goes through `escaped`, so a control byte cannot move the cursor or forge a line.
fn write_text(history: &History, limit: Limit, log: &str, out: &mut dyn Write) -> io::Result<()> {
    let e = |text: &str| escaped(OsStr::new(text)).to_string();
    writeln!(out, "\nDustpan history\n\nRecent sessions")?;
    let mut sessions = history.sessions(limit).peekable();
    if sessions.peek().is_none() {
        writeln!(out, "  No operation history yet.")?;
    }
    for s in sessions {
        let a = &s.actions;
        let counts: Vec<String> = [
            ("removed", a.removed),
            ("trashed", a.trashed),
            ("skipped", a.skipped),
            ("failed", a.failed),
            ("rebuilt", a.rebuilt),
            ("other", a.other),
        ]
        .iter()
        .filter(|(_, n)| *n > 0)
        .map(|(word, n)| format!("{word} {n}"))
        .collect();
        let counts = if counts.is_empty() {
            "no file actions".to_owned()
        } else {
            counts.join(", ")
        };
        let ended = if s.ended_at.is_empty() {
            "not ended".to_owned()
        } else {
            e(&s.ended_at)
        };
        writeln!(
            out,
            "  {:<10} {}, {} items, {}",
            e(&s.command),
            e(&s.started_at),
            s.items,
            e(&s.size)
        )?;
        writeln!(out, "             {counts}, ended {ended}")?;
    }
    writeln!(out, "\nDeletion audit")?;
    let mut deletions = history.deletions(limit).peekable();
    if deletions.peek().is_none() {
        writeln!(out, "  No deletion audit entries yet.")?;
    }
    for d in deletions {
        let size = d.size_kb.map_or_else(
            || "unknown".to_owned(),
            |kb| Size::Known(kb.saturating_mul(1024)).to_string(),
        );
        writeln!(
            out,
            "  {:<24} {:<9} {:<16} {:>8}  {}",
            e(&d.timestamp),
            d.mode,
            e(&d.status),
            size,
            e(&d.path)
        )?;
    }
    writeln!(
        out,
        "\nLogs\n  operations: {0}\n  deletions:  {0}\n",
        e(log)
    )
}
