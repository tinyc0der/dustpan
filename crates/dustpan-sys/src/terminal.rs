pub const fn console_is_utf8() -> bool {
    cfg!(windows)
}

/// Runs `handler` on a helper thread when the process gets SIGTERM, SIGHUP or SIGINT, or when its controlling
/// terminal disappears without a SIGHUP ever arriving. The handler may run on either of two threads, so it must
/// not return. A caller blocked in crossterm's `poll` never sees either event: its tty read loop spins on EOF
/// without checking the timeout.
#[cfg(unix)]
pub fn on_termination(handler: impl Fn() + Send + Sync + 'static) -> std::io::Result<()> {
    use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
    use std::sync::Arc;
    use std::time::Duration;

    const TERMINAL_CHECK_TICK: Duration = Duration::from_millis(100);

    let handler = Arc::new(handler);
    let mut signals = signal_hook::iterator::Signals::new([SIGTERM, SIGHUP, SIGINT])?;
    let on_signal = Arc::clone(&handler);
    std::thread::spawn(move || {
        if signals.forever().next().is_some() {
            on_signal();
        }
    });
    if stdin_is_terminal() {
        std::thread::spawn(move || {
            while !terminal_hung_up() {
                std::thread::sleep(TERMINAL_CHECK_TICK);
            }
            handler();
        });
    }
    Ok(())
}

#[cfg(not(unix))]
pub fn on_termination(_handler: impl Fn() + Send + Sync + 'static) -> std::io::Result<()> {
    Ok(())
}

/// `tcgetattr` and `ioctl` still succeed on a hung-up terminal, and macOS poll reports nothing unless POLLIN is requested.
#[cfg(unix)]
fn terminal_hung_up() -> bool {
    let mut fd = libc::pollfd {
        fd: libc::STDIN_FILENO,
        events: libc::POLLIN,
        revents: 0,
    };
    // SAFETY: `fd` is one valid pollfd and the count is 1.
    let ready = unsafe { libc::poll(&mut fd, 1, 0) };
    ready > 0 && fd.revents & libc::POLLHUP != 0
}

#[cfg(unix)]
fn stdin_is_terminal() -> bool {
    use std::io::IsTerminal;
    std::io::stdin().is_terminal()
}
