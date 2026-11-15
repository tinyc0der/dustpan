use std::process::ExitCode;

/// The one table of exit codes. Everything that ends the process goes through it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exit {
    Done = 0,
    Failed = 1,
    Usage = 2,
    Refused = 3,
    Deadline = 124,
    Interrupted = 130,
}

impl From<Exit> for ExitCode {
    fn from(exit: Exit) -> Self {
        ExitCode::from(exit as u8)
    }
}
