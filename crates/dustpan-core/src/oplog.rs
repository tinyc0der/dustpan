//! The operations log: one row per removal, Trash move, refusal and failure, in Mole's grammar
//! (SAFE-22), so `history`, Mole's own reader and a support diagnostic read the same file.
//!
//! ```text
//!
//! # ========== clean run=20261116094500-4242-9f3a session started at 2026-11-16 09:45:00 +0700 ==========
//! [2026-11-16 09:45:01] [clean run=20261116094500-4242-9f3a] REMOVED /Users/a/Library/Caches/x (sink=delete size=1500000)
//! [2026-11-16 09:45:01] [clean run=20261116094500-4242-9f3a] SKIPPED /Users/a/Library/Caches/y (sink=trash reason=owner-running)
//! # ========== clean run=20261116094500-4242-9f3a session ended at 2026-11-16 09:45:02, 1 items, 1.5 MB ==========
//! ```
//!
//! The start marker adds the local UTC offset after Mole's stamp, so `history` can print each
//! item's time as Mole's `deletions.log` does (`%Y-%m-%dT%H:%M:%S%z`). Mole's reader keeps the
//! whole tail as the start time.
//!
//! The row model and the grammar live here, in one place, for writing and for reading back.
//! The file lives behind [`LogStore`], which `dustpan-sys` implements; the rotation and
//! warn-once policy is [`OpLog`], so it is tested without a file system.
//!
//! A crash leaves at worst one torn last line and no end marker. Each line is one append,
//! written synchronously after the item's outcome is known. The next session's start marker
//! begins with a newline, so a torn line stays a line of its own that the reader skips, and
//! `history` shows the crashed session as not ended. Nothing is rewritten, so reading never
//! needs repair (SAFE-37).
use crate::path::{AbsPath, escaped};
use crate::size::Size;
use crate::time::{WallTime, local_stamp, utc_offset};
use std::fmt;
use std::io::{self, Write as _};

/// Mole rotates `operations.log` to `operations.log.old` above 5 MB, checked once per session.
pub const ROTATE_AT: u64 = 5 * 1024 * 1024;

/// The command word in `[<command> run=<id>]`: a closed set of constants plus
/// [`Command::named`] for a caller that is not a CLI command, such as the sink integration test.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Command(&'static str);

impl Command {
    pub const CLEAN: Self = Self("clean");
    pub const UNINSTALL: Self = Self("uninstall");
    pub const PURGE: Self = Self("purge");
    pub const INSTALLER: Self = Self("installer");
    pub const ANALYZE: Self = Self("analyze");

    /// Lowercase ASCII letters and `-` only, so the word cannot break the bracket grammar.
    pub const fn named(word: &'static str) -> Self {
        let bytes = word.as_bytes();
        assert!(!bytes.is_empty(), "a command word is not empty");
        let mut i = 0;
        while i < bytes.len() {
            assert!(
                bytes[i].is_ascii_lowercase() || bytes[i] == b'-',
                "a command word is lowercase letters and `-`"
            );
            i += 1;
        }
        Self(word)
    }

    pub const fn word(self) -> &'static str {
        self.0
    }
}

/// `<YYYYMMDDhhmmss>-<pid>-<nonce hex>`, inside Mole's run id alphabet `[A-Za-z0-9._-]+`. The
/// nonce comes from the caller, since core reads no randomness.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RunId(Box<str>);

impl RunId {
    pub fn new(at: WallTime, pid: u32, nonce: u64) -> Self {
        let digits: String = local_stamp(at)
            .chars()
            .filter(char::is_ascii_digit)
            .collect();
        Self(format!("{digits}-{pid}-{nonce:x}").into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One run of one command. A preview names its command `<command>-dry-run`, as Mole does, so a
/// preview never reads as a real run. Built only by the executor, from its own mode, so the
/// session name and what the run does cannot disagree.
#[derive(Clone, Debug)]
pub struct Session {
    command: Command,
    run: RunId,
    dry_run: bool,
}

impl Session {
    pub(crate) fn new(command: Command, run: RunId, dry_run: bool) -> Self {
        Self {
            command,
            run,
            dry_run,
        }
    }

    pub fn run(&self) -> &RunId {
        &self.run
    }
}

/// Mole's action words. `REBUILT` and `TASK_FAILED` belong to optimize and arrive with it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Removed,
    Trashed,
    Skipped,
    Failed,
}

impl Action {
    pub const fn word(self) -> &'static str {
        match self {
            Self::Removed => "REMOVED",
            Self::Trashed => "TRASHED",
            Self::Skipped => "SKIPPED",
            Self::Failed => "FAILED",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SinkWord {
    Delete,
    Trash,
}

/// The `(detail)` of an item row: `sink=delete size=1500000 reason=owner-running inner=...`,
/// fields in that order, absent ones left out. Every value is a code, a number, `N+`,
/// `unknown`, or a path with its spaces and parentheses escaped (`to=`), so none holds a space
/// or a parenthesis, and a reader splits a row at its last ` (` however the path is spelled.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Detail<'a> {
    pub sink: Option<SinkWord>,
    /// Allocated bytes freed by a removal, or moved by a Trash move.
    pub size: Option<Size>,
    /// A `KeptReason::code`, a `FailCause::code`, `gone` or `trashed-other`.
    pub reason: Option<&'static str>,
    /// For `holds-kept`, the code of what is kept inside.
    pub inner: Option<&'static str>,
    /// For a Trash move, where the item landed, when the row must say it (`trashed-other`).
    pub to: Option<&'a AbsPath>,
}

#[derive(Clone, Copy, Debug)]
pub struct Row<'a> {
    pub at: WallTime,
    pub action: Action,
    pub path: &'a AbsPath,
    pub detail: Detail<'a>,
}

/// One line of the log.
#[derive(Clone, Copy, Debug)]
pub enum Line<'a> {
    Start {
        session: &'a Session,
        at: WallTime,
    },
    Item {
        session: &'a Session,
        row: Row<'a>,
    },
    /// `items` counts removals and Trash moves; `freed` counts removals only (SAFE-21).
    End {
        session: &'a Session,
        at: WallTime,
        items: u64,
        freed: Size,
    },
}

/// Where the executor sends lines. [`OpLog`] writes the file. The privileged helper (M4) passes
/// one that sends `Item` rows over its pipe and drops `Start` and `End`, so the parent writes
/// every line of one session into one file it owns.
///
/// Infallible on purpose: a log failure never stops a delete (SAFE-22). The implementation
/// reports trouble once, through the callback it was built with.
pub trait Journal {
    fn write(&mut self, line: Line<'_>);
}

/// The log file, as two calls a test can fake. `dustpan_sys::FileStore` keeps
/// `operations.log` in the folder `paths::log_dir` names.
pub trait LogStore {
    /// Renames the current file to `operations.log.old` when it is larger than `limit`, with
    /// the size read again while holding a lock, so two runs that start together rotate once.
    fn rotate_above(&mut self, limit: u64) -> io::Result<()>;
    /// Appends one or more complete lines with one write.
    fn append(&mut self, bytes: &[u8]) -> io::Result<()>;
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LogStep {
    Rotate,
    Append,
}

/// The one warning a session prints when its log cannot be written. The binary prints it to
/// stderr; the TUI shows it on the progress screen.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LogWarning {
    pub step: LogStep,
    pub error: io::ErrorKind,
}

impl fmt::Display for LogWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let step = match self.step {
            LogStep::Rotate => "rotate",
            LogStep::Append => "write",
        };
        write!(
            f,
            "cannot {step} the operations log ({}); the run goes on without it",
            self.error
        )
    }
}

/// The rotation and warn-once policy over a [`LogStore`]. Rotation is checked at `Start` only,
/// so a session's rows never straddle the two files unless the session itself passes 5 MB.
pub struct OpLog<S, W> {
    store: S,
    warn: W,
    warned: bool,
    buf: Vec<u8>,
}

impl<S: LogStore, W: FnMut(&LogWarning)> OpLog<S, W> {
    pub fn new(store: S, warn: W) -> Self {
        Self {
            store,
            warn,
            warned: false,
            buf: Vec::new(),
        }
    }

    /// The store back, for a test to read what was written.
    pub fn into_store(self) -> S {
        self.store
    }

    fn trouble(&mut self, step: LogStep, error: &io::Error) {
        if !self.warned {
            self.warned = true;
            (self.warn)(&LogWarning {
                step,
                error: error.kind(),
            });
        }
    }
}

impl<S: LogStore, W: FnMut(&LogWarning)> Journal for OpLog<S, W> {
    fn write(&mut self, line: Line<'_>) {
        if let Line::Start { .. } = line
            && let Err(e) = self.store.rotate_above(ROTATE_AT)
        {
            self.trouble(LogStep::Rotate, &e);
        }
        let mut buf = std::mem::take(&mut self.buf);
        buf.clear();
        format(&line, &mut buf);
        if let Err(e) = self.store.append(&buf) {
            self.trouble(LogStep::Append, &e);
        }
        self.buf = buf;
    }
}

fn command_field(session: &Session) -> String {
    let dry = if session.dry_run { "-dry-run" } else { "" };
    format!(
        "{}{dry} run={}",
        session.command.word(),
        session.run.as_str()
    )
}

fn detail_field(detail: &Detail<'_>) -> String {
    let mut fields: Vec<String> = Vec::with_capacity(4);
    if let Some(sink) = detail.sink {
        fields.push(match sink {
            SinkWord::Delete => "sink=delete".into(),
            SinkWord::Trash => "sink=trash".into(),
        });
    }
    if let Some(size) = detail.size {
        fields.push(match size {
            Size::Known(bytes) => format!("size={bytes}"),
            Size::Partial(bytes) => format!("size={bytes}+"),
            Size::Unknown => "size=unknown".into(),
        });
    }
    if let Some(reason) = detail.reason {
        fields.push(format!("reason={reason}"));
    }
    if let Some(inner) = detail.inner {
        fields.push(format!("inner={inner}"));
    }
    if let Some(to) = detail.to {
        fields.push(format!("to={}", detail_value(to)));
    }
    fields.join(" ")
}

/// A path as a detail value: escaped like a row's path, and with every space and parenthesis
/// written as `\xNN`, so the value holds neither and the row still splits at its last ` (`.
fn detail_value(path: &AbsPath) -> String {
    let mut out = String::new();
    for c in escaped(path.as_path().as_os_str()).to_string().chars() {
        match c {
            ' ' | '(' | ')' => out.push_str(&format!("\\x{:02X}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Writes one line in Mole's grammar. The path goes through `escaped`, the one escaping rule,
/// so a newline in a name prints as `\n` and cannot start a forged row.
pub(crate) fn format(line: &Line<'_>, out: &mut Vec<u8>) {
    // Writing to a `Vec` cannot fail.
    let _ = match line {
        Line::Start { session, at } => write!(
            out,
            "\n# ========== {} session started at {} {} ==========\n",
            command_field(session),
            local_stamp(*at),
            utc_offset(*at)
        ),
        Line::Item { session, row } => writeln!(
            out,
            "[{}] [{}] {} {} ({})",
            local_stamp(row.at),
            command_field(session),
            row.action.word(),
            escaped(row.path.as_path().as_os_str()),
            detail_field(&row.detail)
        ),
        Line::End {
            session,
            at,
            items,
            freed,
        } => writeln!(
            out,
            "# ========== {} session ended at {}, {items} items, {freed} ==========",
            command_field(session),
            local_stamp(*at)
        ),
    };
}

/// A line read back, borrowed from the log's bytes. Fields are bytes because the file is
/// user-writable and may hold anything; `history` decodes them, invalid UTF-8 as U+FFFD.
#[derive(Debug)]
pub(crate) enum Parsed<'a> {
    Start {
        command: &'a [u8],
        run: Option<&'a [u8]>,
        at: &'a [u8],
    },
    End {
        command: &'a [u8],
        run: Option<&'a [u8]>,
        at: &'a [u8],
        items: Option<u64>,
        size: &'a [u8],
    },
    Item {
        at: &'a [u8],
        command: &'a [u8],
        run: Option<&'a [u8]>,
        action: &'a [u8],
        path: &'a [u8],
        detail: Fields<'a>,
    },
}

/// The `key=value` pairs of a detail. A detail Mole wrote (`(15.2MB)`, `(whitelist)`) has none,
/// and its row still counts by its action.
#[derive(Debug, Default)]
pub(crate) struct Fields<'a> {
    pub sink: Option<&'a [u8]>,
    pub size: Option<&'a [u8]>,
    pub reason: Option<&'a [u8]>,
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn rfind(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).rposition(|w| w == needle)
}

/// `<command>` or `<command> run=<id>`, with Mole's id alphabet `[A-Za-z0-9._-]+`.
fn command_run(field: &[u8]) -> Option<(&[u8], Option<&[u8]>)> {
    let (command, run) = match find(field, b" run=") {
        Some(i) => (&field[..i], Some(&field[i + 5..])),
        None => (field, None),
    };
    let id_ok = |id: &[u8]| {
        !id.is_empty()
            && id
                .iter()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
    };
    (!command.is_empty() && run.is_none_or(id_ok)).then_some((command, run))
}

fn fields(detail: &[u8]) -> Fields<'_> {
    let mut out = Fields::default();
    for pair in detail.split(|&b| b == b' ') {
        let Some(eq) = pair.iter().position(|&b| b == b'=') else {
            continue;
        };
        let value = Some(&pair[eq + 1..]);
        match &pair[..eq] {
            b"sink" => out.sink = value,
            b"size" => out.size = value,
            b"reason" => out.reason = value,
            _ => {}
        }
    }
    out
}

/// Parses one line without its newline, the way Mole's `history.sh` does. `None` for a blank
/// line, a torn line, or anything that is not a marker or a row, which the reader skips.
pub(crate) fn parse(line: &[u8]) -> Option<Parsed<'_>> {
    const STARTED: &[u8] = b" session started at ";
    const ENDED: &[u8] = b" session ended at ";
    if let Some(marker) = line.strip_prefix(b"# ========== ") {
        let inner = marker.strip_suffix(b" ==========")?;
        if let Some(i) = find(inner, STARTED) {
            let (command, run) = command_run(&inner[..i])?;
            let at = &inner[i + STARTED.len()..];
            return Some(Parsed::Start { command, run, at });
        }
        let i = find(inner, ENDED)?;
        let (command, run) = command_run(&inner[..i])?;
        let rest = &inner[i + ENDED.len()..];
        let (at, items, size) = match find(rest, b", ") {
            Some(comma) => {
                let tail = &rest[comma + 2..];
                match find(tail, b" items, ") {
                    Some(word) => (
                        &rest[..comma],
                        std::str::from_utf8(&tail[..word])
                            .ok()
                            .and_then(|n| n.parse().ok()),
                        &tail[word + 8..],
                    ),
                    None => (&rest[..comma], None, &b""[..]),
                }
            }
            None => (rest, None, &b""[..]),
        };
        return Some(Parsed::End {
            command,
            run,
            at,
            items,
            size,
        });
    }
    let rest = line.strip_prefix(b"[")?;
    let close = find(rest, b"] [")?;
    let at = &rest[..close];
    let rest = &rest[close + 3..];
    let close = find(rest, b"] ")?;
    let (command, run) = command_run(&rest[..close])?;
    let rest = &rest[close + 2..];
    let space = rest.iter().position(|&b| b == b' ')?;
    let (action, rest) = (&rest[..space], &rest[space + 1..]);
    if at.is_empty() || action.is_empty() {
        return None;
    }
    let (path, detail) = match (rest.strip_suffix(b")"), rfind(rest, b" (")) {
        (Some(inside), Some(open)) => (&rest[..open], fields(&inside[open + 2..])),
        _ => (rest, Fields::default()),
    };
    Some(Parsed::Item {
        at,
        command,
        run,
        action,
        path,
        detail,
    })
}

/// An in-memory log for tests: rotation, failure and the bytes written are all visible.
#[cfg(any(test, feature = "testkit"))]
#[derive(Default)]
pub struct MemoryStore {
    pub current: Vec<u8>,
    pub old: Vec<u8>,
    pub rotations: u32,
    /// Every call fails with this, as a read-only log folder does.
    pub failing: Option<io::ErrorKind>,
}

#[cfg(any(test, feature = "testkit"))]
impl LogStore for MemoryStore {
    fn rotate_above(&mut self, limit: u64) -> io::Result<()> {
        if let Some(kind) = self.failing {
            return Err(kind.into());
        }
        if self.current.len() as u64 > limit {
            self.old = std::mem::take(&mut self.current);
            self.rotations += 1;
        }
        Ok(())
    }

    fn append(&mut self, bytes: &[u8]) -> io::Result<()> {
        if let Some(kind) = self.failing {
            return Err(kind.into());
        }
        self.current.extend_from_slice(bytes);
        Ok(())
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::history::{History, Limit};

    fn at() -> WallTime {
        WallTime {
            unix_secs: 1_763_280_000,
            utc_offset_secs: 7 * 3_600,
        }
    }

    fn session() -> Session {
        Session::new(Command::CLEAN, RunId::new(at(), 4242, 0x9f3a), false)
    }

    #[test]
    fn a_row_reads_back_whatever_its_path_holds() {
        let path = AbsPath::parse("/Users/a/Library/Caches/x (1) (copy)".as_ref()).unwrap();
        let session = session();
        let mut out = Vec::new();
        let row = Row {
            at: at(),
            action: Action::Removed,
            path: &path,
            detail: Detail {
                sink: Some(SinkWord::Delete),
                size: Some(Size::Known(1_500_000)),
                ..Detail::default()
            },
        };
        for line in [
            Line::Start {
                session: &session,
                at: at(),
            },
            Line::Item {
                session: &session,
                row,
            },
            Line::End {
                session: &session,
                at: at(),
                items: 1,
                freed: Size::Known(1_500_000),
            },
        ] {
            format(&line, &mut out);
        }
        let text = String::from_utf8_lossy(&out);
        assert!(
            text.contains(
                "\n# ========== clean run=20251116150000-4242-9f3a session started at \
                 2025-11-16 15:00:00 +0700 ==========\n[2025-11-16 15:00:00] \
                 [clean run=20251116150000-4242-9f3a] REMOVED /Users/a/Library/Caches/x (1) \
                 (copy) (sink=delete size=1500000)\n"
            ),
            "{text}"
        );
        let history = History::read(b"", &out);
        let record = history.deletions(Limit::DEFAULT).next().unwrap();
        assert_eq!(record.path, "/Users/a/Library/Caches/x (1) (copy)");
        assert_eq!(record.timestamp, "2025-11-16T15:00:00+0700");
        assert_eq!((record.mode, record.status.as_str()), ("permanent", "ok"));
        assert_eq!(record.size_kb, Some(1_465));
        let session = history.sessions(Limit::DEFAULT).next().unwrap();
        assert_eq!(session.started_at, "2025-11-16 15:00:00");
        assert_eq!((session.items, session.size.as_str()), (1, "1.5 MB"));
        assert_eq!(session.actions.removed, 1);
    }
}
