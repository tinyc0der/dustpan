mod cli;
mod debug;
mod exit;

use exit::Exit;
use std::ffi::OsString;
use std::io::Write;
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

    // A closed pipe is not an error: write and ignore failure instead of panicking in print macros.
    if cli.version {
        let _ = writeln!(
            std::io::stdout(),
            "dustpan {}\n{facts}",
            env!("CARGO_PKG_VERSION")
        );
    } else {
        let _ = cmd.print_help();
    }
    Exit::Done.into()
}
