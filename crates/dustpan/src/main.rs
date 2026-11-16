#![forbid(unsafe_code)]

mod cli;
mod cmd;
mod config;
mod debug;
mod exit;

use dustpan::tui;
use dustpan_core::history::Limit;
use exit::Exit;
use std::ffi::OsString;
use std::io::{IsTerminal, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    // SAFE-16. This runs before argument parsing so even `--version` refuses.
    if dustpan_sys::is_root() {
        let _ = writeln!(
            std::io::stderr(),
            "dustpan: run without sudo; it asks for administrator access when an operation needs it."
        );
        return Exit::Refused.into();
    }

    let args: Vec<OsString> = std::env::args_os().collect();
    let mut cmd = cli::command(cli::wants_no_color(&args, std::env::var_os("NO_COLOR")));
    let cli = match cli::parse(&mut cmd, &args) {
        Ok(cli) => cli,
        Err(err) => {
            let _ = err.print();
            return if err.use_stderr() {
                Exit::Usage
            } else {
                Exit::Done
            }
            .into();
        }
    };

    let facts = dustpan_sys::os_facts();
    if cli.debug {
        match debug::start_session(&facts, &args) {
            Ok(path) => _ = writeln!(std::io::stderr(), "debug log: {path}"),
            Err(err) => {
                _ = writeln!(
                    std::io::stderr(),
                    "dustpan: could not write the debug log: {err}"
                )
            }
        }
    }

    match &cli.command {
        Some(cli::Cmd::Explain { json, paths }) => {
            return cmd::explain::run(paths, *json, &mut std::io::stdout().lock()).into();
        }
        Some(cli::Cmd::History { json, limit }) => {
            let limit = Limit::new(u64::from(*limit)).unwrap_or(Limit::DEFAULT);
            return cmd::history::run(*json, limit, &mut std::io::stdout().lock()).into();
        }
        None => {}
    }

    // A closed pipe is not an error: write and ignore failure instead of panicking in print macros.
    if cli.version {
        let _ = writeln!(
            std::io::stdout(),
            "dustpan {}\n{facts}",
            env!("CARGO_PKG_VERSION")
        );
    } else if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
        let options = tui::Options {
            ascii: cli.ascii,
            no_color: cli::wants_no_color(&args, std::env::var_os("NO_COLOR")),
            interrupted_exit: Exit::Interrupted as i32,
        };
        return match tui::run(options) {
            Ok(tui::Finish::Quit) => Exit::Done,
            Ok(tui::Finish::Interrupted) => Exit::Interrupted,
            Err(err) => {
                _ = writeln!(std::io::stderr(), "dustpan: the terminal failed: {err}");
                Exit::Failed
            }
        }
        .into();
    } else {
        let _ = cmd.print_help();
    }
    Exit::Done.into()
}
