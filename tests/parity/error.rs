use std::fmt;
use std::io;

pub const SUPPORTED_MOLE: &[&str] = &["1.63.0", "1.67.0"];

#[derive(Debug)]
pub enum Error {
    Io(String, io::Error),
    Manifest(String),
    MoleMissing(String),
    MoleVersion { found: String },
    MoleFailed { what: String, detail: String },
    Parse(ParseError),
    Sandbox(String),
    Escaped(Vec<String>),
    Drift(String),
    Interrupted,
    FixtureOutsideTemp(String),
    Usage(String),
}

#[derive(Debug, PartialEq, Eq)]
pub struct ParseError {
    pub source: &'static str,
    pub line_no: usize,
    pub line: String,
    pub expected: &'static str,
}

impl Error {
    pub fn exit_code(&self) -> i32 {
        match self {
            Error::MoleMissing(_) => 3,
            Error::MoleVersion { .. } => 4,
            Error::Parse(_) => 5,
            Error::Sandbox(_) | Error::FixtureOutsideTemp(_) => 6,
            Error::Usage(_) => 2,
            Error::Escaped(_) => 7,
            Error::Interrupted => 130,
            _ => 1,
        }
    }
}

pub fn io_err(what: impl Into<String>) -> impl FnOnce(io::Error) -> Error {
    let what = what.into();
    move |e| Error::Io(what, e)
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unrecognised line {} in {}: {:?} (expected {}). The parsers were written against Mole {}; \
             rerun with a verified Mole or update the parser",
            self.line_no,
            self.source,
            self.line,
            self.expected,
            SUPPORTED_MOLE.join(" and "),
        )
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(what, e) => write!(f, "{what}: {e}"),
            Error::Manifest(m) => write!(f, "bad manifest: {m}"),
            Error::MoleMissing(m) => write!(
                f,
                "{m}\nInstall Mole with `brew install mole` (https://github.com/tw93/Mole), \
                 or point PARITY_MO at the `mo` binary."
            ),
            Error::MoleVersion { found } => write!(
                f,
                "Mole {found} is not a verified version. Verified: {}. Install one of them, \
                 or add this version to SUPPORTED_MOLE after checking the parsers against it.",
                SUPPORTED_MOLE.join(", ")
            ),
            Error::MoleFailed { what, detail } => write!(f, "{what} failed: {detail}"),
            Error::Parse(p) => p.fmt(f),
            Error::Sandbox(m) => write!(f, "sandbox check failed: {m}"),
            Error::FixtureOutsideTemp(p) => write!(
                f,
                "refusing to run: fixture {p} is not under the temp folder, and the harness must never touch the real home"
            ),
            Error::Usage(m) => write!(f, "{m}"),
            Error::Escaped(paths) => write!(
                f,
                "Mole listed {} path(s) under the fixture root but outside the fixture home: {}",
                paths.len(),
                paths.join(", ")
            ),
            Error::Drift(m) => write!(f, "the fixture changed during the run: {m}"),
            Error::Interrupted => {
                write!(f, "interrupted; Mole was stopped and the fixture removed")
            }
        }
    }
}

impl std::error::Error for Error {}

impl From<ParseError> for Error {
    fn from(p: ParseError) -> Self {
        Error::Parse(p)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
