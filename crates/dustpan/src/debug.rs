use dustpan_core::facts::OsFacts;
use dustpan_core::paths::{self, Os};
use dustpan_core::time::rfc3339_utc;
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::time::{SystemTime, UNIX_EPOCH};

/// Writes the session log and returns its home-redacted path.
pub fn start_session(facts: &OsFacts, args: &[OsString]) -> io::Result<String> {
    let env = |key: &str| std::env::var(key).ok();
    let os = Os::current();
    let dir = paths::log_dir(os, &env)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no home folder to log under"))?;
    let home = paths::home_dir(os, &env);
    let redact = |text: &str| paths::redact_home(text, home.as_deref());

    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    fs::create_dir_all(&dir)?;
    let file = dir.join(format!("session-{secs}-{}.log", std::process::id()));
    let argv: Vec<String> = args.iter().map(|a| redact(&a.to_string_lossy())).collect();

    let mut log = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&file)?;
    writeln!(log, "dustpan {}", env!("CARGO_PKG_VERSION"))?;
    writeln!(log, "{facts}")?;
    writeln!(log, "argv: {}", argv.join(" "))?;
    writeln!(log, "started: {}", rfc3339_utc(secs))?;
    Ok(redact(&file.to_string_lossy()))
}
