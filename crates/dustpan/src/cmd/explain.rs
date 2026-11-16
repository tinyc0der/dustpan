//! `dustpan explain PATH... [--json]`: the guard's verdict for each path and the layer that
//! decided it. Read-only. Exit 0 when every path was judged (a `Kept` verdict is an answer, not
//! a failure), 1 when the guard cannot be built, 2 for usage errors.
use crate::exit::Exit;
use dustpan_core::guard::{Guard, GuardInputs, KeptReason, ShapeReason, Verdict};
use dustpan_core::path::{AbsPath, escaped};
use dustpan_core::paths::{self, Os};
use dustpan_core::probe::{self, Probe, Seen};
use dustpan_sys::LiveProbe;
use std::ffi::{OsStr, OsString};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// One printed row. `path` is already escaped (SAFE-22), so a newline in a name prints as `\n`.
struct Row {
    path: String,
    verdict: Verdict,
    /// After a `symlink-ancestor` refusal: the physical spelling the user can explain instead.
    physical: Option<AbsPath>,
}

fn fail(message: std::fmt::Arguments<'_>, exit: Exit) -> Exit {
    let _ = writeln!(io::stderr(), "dustpan: {message}");
    exit
}

pub fn run(args: &[OsString], json: bool, out: &mut dyn Write) -> Exit {
    let env = |key: &str| std::env::var(key).ok();
    let os = Os::current();
    let probe = LiveProbe::new();
    let home = paths::home_dir(os, &env).and_then(|h| AbsPath::parse(h.as_os_str()).ok());
    let Some(home) = home else {
        return fail(
            format_args!(
                "the home folder is not set to an absolute path, so no path can be judged"
            ),
            Exit::Failed,
        );
    };
    // The guard refuses every path below a link, so a home behind one (`/home` on Fedora
    // Atomic) is named by its physical path before any `~` rule is anchored on it.
    let home = match probe::physical(&probe, &home) {
        Seen::Found(physical) => physical,
        Seen::Missing => home,
        Seen::Unknown(why) => {
            return fail(
                format_args!("cannot inspect the home folder {home}: {}", why.sentence()),
                Exit::Failed,
            );
        }
    };
    let inputs = GuardInputs {
        os,
        home: home.clone(),
        env: &env,
    };
    let guard = match Guard::new(inputs, &probe) {
        Ok(guard) => guard,
        Err(e) => {
            return fail(
                format_args!("cannot load the protection policy: {e}"),
                Exit::Failed,
            );
        }
    };

    let cwd = std::env::current_dir().ok();
    let mut pass = guard.pass(&probe);
    let mut rows = Vec::with_capacity(args.len());
    for arg in args {
        let Some(typed) = absolute(arg, &home, cwd.as_deref()) else {
            return fail(
                format_args!(
                    "cannot resolve {}: the working folder is gone; give an absolute path",
                    escaped(arg)
                ),
                Exit::Usage,
            );
        };
        rows.push(match AbsPath::parse(typed.as_os_str()) {
            Ok(path) => {
                let judgment = pass.evaluate(path);
                let physical = match judgment.verdict() {
                    Verdict::Kept(KeptReason::Shape(ShapeReason::LinkedAncestor { .. })) => {
                        physical_spelling(&probe, judgment.path())
                    }
                    _ => None,
                };
                Row {
                    path: judgment.path().to_string(),
                    verdict: judgment.verdict().clone(),
                    physical,
                }
            }
            Err(e) => Row {
                path: escaped(typed.as_os_str()).to_string(),
                verdict: Verdict::Kept(KeptReason::Shape(ShapeReason::Malformed(e))),
                physical: None,
            },
        });
    }

    // A closed pipe is not a failure of the command (M0-1's rule for `--version`).
    let _ = if json {
        write_json(out, &rows)
    } else {
        write_text(out, &rows)
    };
    Exit::Done
}

/// The path the user meant. A leading `~` is the home folder, and a relative path starts at the
/// working folder. Resolved here, at the CLI boundary: the guard only ever sees an absolute
/// path, and `AbsPath::parse` refuses a relative one (SAFE-02).
fn absolute(arg: &OsStr, home: &AbsPath, cwd: Option<&Path>) -> Option<PathBuf> {
    let typed = Path::new(arg);
    if let Ok(rest) = typed.strip_prefix("~") {
        Some(home.as_path().join(rest))
    } else if typed.is_absolute() || arg.is_empty() {
        Some(typed.to_path_buf())
    } else {
        Some(cwd?.join(typed))
    }
}

fn physical_spelling(probe: &dyn Probe, path: &AbsPath) -> Option<AbsPath> {
    match probe::physical(probe, path) {
        Seen::Found(physical) => Some(physical),
        Seen::Missing | Seen::Unknown(_) => None,
    }
}

/// `/System  Kept · critical system path`
fn write_text(out: &mut dyn Write, rows: &[Row]) -> io::Result<()> {
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
/// eligible path. `source` names the user file behind a refusal, which only the whitelist layer
/// (M0-4) has. The keys are written in this order by hand, so the output reads the same in
/// every run; M0-5 moves the envelope into core.
fn write_json(out: &mut dyn Write, rows: &[Row]) -> io::Result<()> {
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
            quoted_or_null(source.map(ToString::to_string)),
        )?;
    }
    writeln!(out, "]}}")
}
