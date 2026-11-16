use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::flag;
use std::io;
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

static FLAG: OnceLock<Arc<AtomicBool>> = OnceLock::new();

/// Turns SIGINT and SIGTERM into a flag the runner polls, so a stop still
/// kills Mole's process group and drops the fixture on the way out.
pub fn install() -> io::Result<()> {
    let flag_arc = FLAG.get_or_init(|| Arc::new(AtomicBool::new(false)));
    for signal in [SIGINT, SIGTERM] {
        flag::register(signal, Arc::clone(flag_arc))?;
    }
    Ok(())
}

pub fn requested() -> bool {
    FLAG.get().is_some_and(|f| f.load(Ordering::Relaxed))
}
