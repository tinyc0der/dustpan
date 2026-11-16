use clap::{ColorChoice, Command, CommandFactory, FromArgMatches, Parser};
use std::ffi::OsString;

#[derive(Parser, Debug)]
#[command(
    name = "dustpan",
    about = "A disk cleaner with a terminal UI and a scriptable CLI",
    disable_version_flag = true
)]
pub struct Cli {
    /// Print version and system details
    #[arg(short = 'V', long)]
    pub version: bool,
    /// Write a session log to the dustpan log folder
    #[arg(long, global = true)]
    pub debug: bool,
    /// Draw with plain ASCII characters
    #[arg(long, global = true)]
    pub ascii: bool,
    /// Turn off color
    #[arg(long = "no-color", global = true)]
    pub no_color: bool,
}

/// `--help` prints while clap parses, so color must be decided before parsing.
pub fn wants_no_color(args: &[OsString], no_color_env: Option<OsString>) -> bool {
    no_color_env.is_some_and(|v| !v.is_empty()) || args.iter().any(|a| a == "--no-color")
}

pub fn command(no_color: bool) -> Command {
    let cmd = Cli::command();
    if no_color {
        cmd.color(ColorChoice::Never)
    } else {
        cmd
    }
}

pub fn parse(cmd: &mut Command, args: &[OsString]) -> Result<Cli, clap::Error> {
    Cli::from_arg_matches(&cmd.try_get_matches_from_mut(args)?)
}
