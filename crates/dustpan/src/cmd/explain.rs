//! `dustpan explain PATH... [--json]`: the guard's verdict for each path and the layer that
//! decided it. Read-only. Exit 0 when every path was judged (a `Kept` verdict is an answer, not
//! a failure), 1 when the whitelist cannot be read or the guard cannot be built, 2 for usage
//! errors.
use crate::exit::Exit;
use dustpan::check::{Checked, Checker};
use dustpan_core::guard::KeptReason;
use std::ffi::OsString;
use std::io::{self, Write};
use std::time::{SystemTime, UNIX_EPOCH};

fn fail(message: std::fmt::Arguments<'_>, exit: Exit) -> Exit {
    let _ = writeln!(io::stderr(), "dustpan: {message}");
    exit
}

pub fn run(args: &[OsString], json: bool, out: &mut dyn Write) -> Exit {
    let env = |key: &str| std::env::var(key).ok();
    let checker = match Checker::open(&env) {
        Ok(checker) => checker,
        Err(e) => return fail(format_args!("{e}"), Exit::Failed),
    };
    let mut rows = Vec::with_capacity(args.len());
    for arg in args {
        match checker.check(arg) {
            Ok(row) => rows.push(row),
            Err(e) => return fail(format_args!("{e}"), Exit::Usage),
        }
    }

    // A closed pipe is not a failure of the command (M0-1's rule for `--version`).
    let _ = if json {
        write_json(out, &rows)
    } else {
        write_text(out, &rows)
    };
    Exit::Done
}

/// `/System  Kept · critical system path`
fn write_text(out: &mut dyn Write, rows: &[Checked]) -> io::Result<()> {
    for row in rows {
        writeln!(out, "{}  {}", row.path, row.verdict)?;
        if let Some(physical) = &row.physical {
            writeln!(out, "  physical path: {physical}")?;
        }
    }
    Ok(())
}

fn quoted(text: &str) -> String {
    serde_json::Value::from(text).to_string()
}

fn quoted_or_null(text: Option<String>) -> String {
    text.map_or_else(|| "null".to_owned(), |t| quoted(&t))
}

/// PLAN's envelope with one item per path. `layer`, `reason` and `detail` are null for an
/// eligible path. `source` is the user file behind a whitelist refusal, `default` for a built-in
/// whitelist entry, and null for every other layer. The keys are written in this order by hand, so the output reads the same in
/// every run; M0-5 moves the envelope into core.
fn write_json(out: &mut dyn Write, rows: &[Checked]) -> io::Result<()> {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let run_id = format!("{secs}-{}", std::process::id());
    write!(
        out,
        r#"{{"schema_version":1,"command":"explain","run_id":{},"mode":"read-only","items":["#,
        quoted(&run_id)
    )?;
    for (i, row) in rows.iter().enumerate() {
        let kept = row.verdict.kept();
        let source = kept.and_then(KeptReason::source);
        write!(
            out,
            r#"{}{{"path":{},"verdict":{},"layer":{},"reason":{},"detail":{},"source":{}}}"#,
            if i == 0 { "" } else { "," },
            quoted(&row.path),
            quoted(if kept.is_some() { "kept" } else { "eligible" }),
            quoted_or_null(kept.map(|r| r.layer().code().to_owned())),
            quoted_or_null(kept.map(|r| r.code().to_owned())),
            quoted_or_null(kept.map(ToString::to_string)),
            quoted_or_null(source.map(|s| s.to_string())),
        )?;
    }
    writeln!(out, "]}}")
}
