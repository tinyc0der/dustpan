//! Path shape (guard layer 1, the part that needs no file system).
use std::ffi::{OsStr, OsString};
use std::fmt::{self, Write as _};
use std::path::{Component, Path, PathBuf, Prefix};

/// An absolute, normalized path. It has no `..` component, no control byte, no NUL and no
/// empty name. `//` and `/./` are collapsed and a trailing separator is dropped (SAFE-02,
/// SAFE-03). On Windows it is a drive-letter path. UNC, device and `\\?\` input is refused, and
/// so is a `name:stream` component; `dustpan-sys` adds the `\\?\` prefix itself at the syscall.
///
/// The only constructors are [`AbsPath::parse`] and [`AbsPath::join`], so an `AbsPath` built
/// from an empty variable can never collapse to a parent root (SAFE-04).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct AbsPath(PathBuf);

/// Why raw input is not an [`AbsPath`]. Reported as guard layer 1 (`shape`), never as an error
/// string: `explain` prints it as `Kept`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShapeError {
    Empty,
    Relative,
    ParentComponent,
    ControlByte,
    /// UNC, device namespace, verbatim or alternate-data-stream input on Windows.
    DevicePath,
}

fn has_control_byte(raw: &OsStr) -> bool {
    raw.as_encoded_bytes()
        .iter()
        .any(|&b| b < 0x20 || b == 0x7f)
}

impl AbsPath {
    pub fn parse(raw: &OsStr) -> Result<Self, ShapeError> {
        if raw.is_empty() {
            return Err(ShapeError::Empty);
        }
        if has_control_byte(raw) {
            return Err(ShapeError::ControlByte);
        }
        let mut out = PathBuf::new();
        for component in Path::new(raw).components() {
            match component {
                Component::Prefix(p) => match p.kind() {
                    Prefix::Disk(letter) => {
                        out.push(format!("{}:", char::from(letter.to_ascii_uppercase())));
                    }
                    _ => return Err(ShapeError::DevicePath),
                },
                Component::RootDir => out.push(component.as_os_str()),
                Component::CurDir => {}
                Component::ParentDir => return Err(ShapeError::ParentComponent),
                Component::Normal(name) => {
                    if cfg!(windows) && name.as_encoded_bytes().contains(&b':') {
                        return Err(ShapeError::DevicePath);
                    }
                    out.push(name)
                }
            }
        }
        if out.is_absolute() {
            Ok(Self(out))
        } else {
            Err(ShapeError::Relative)
        }
    }

    /// Cannot produce a root or a parent: a [`Name`] is never empty, `.` or `..`.
    pub fn join(&self, name: &Name) -> Self {
        Self(self.0.join(&name.0))
    }

    pub fn parent(&self) -> Option<Self> {
        self.0.parent().map(|p| Self(p.to_path_buf()))
    }

    pub fn name(&self) -> Option<&OsStr> {
        self.0.file_name()
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }

    /// True when `self` is `other` or lies under it, compared by component.
    pub fn starts_with(&self, other: &AbsPath) -> bool {
        self.0.starts_with(&other.0)
    }

    /// The drive, when there is one, then every name. `/` has none.
    pub(crate) fn names(&self) -> impl Iterator<Item = &OsStr> {
        self.0.components().filter_map(|c| match c {
            Component::Prefix(_) | Component::Normal(_) => Some(c.as_os_str()),
            _ => None,
        })
    }

    /// The root (`/`, or `C:\`) and every ancestor down to `self`, shortest first.
    pub(crate) fn lineage(&self) -> Vec<AbsPath> {
        let mut out: Vec<AbsPath> = self.0.ancestors().map(|p| Self(p.to_path_buf())).collect();
        out.reverse();
        out
    }

    /// Wraps a path that is already known to be absolute and normalized: a prefix of an
    /// `AbsPath`, or one joined with names taken from another `AbsPath`.
    pub(crate) fn trusted(path: PathBuf) -> Self {
        Self(path)
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        self.0.as_os_str().as_encoded_bytes()
    }
}

impl fmt::Display for AbsPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        escaped(self.0.as_os_str()).fmt(f)
    }
}

/// One component as read from a directory: not empty, not `.` or `..`, no separator and no
/// control byte. Scanners build children with [`AbsPath::join`] from it.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Name(OsString);

impl Name {
    pub fn parse(raw: &OsStr) -> Result<Self, ShapeError> {
        if raw.is_empty() {
            return Err(ShapeError::Empty);
        }
        if raw == "." || raw == ".." {
            return Err(ShapeError::ParentComponent);
        }
        if has_control_byte(raw) {
            return Err(ShapeError::ControlByte);
        }
        let bytes = raw.as_encoded_bytes();
        if bytes.contains(&b'/') || (cfg!(windows) && bytes.contains(&b'\\')) {
            return Err(ShapeError::Relative);
        }
        if cfg!(windows) && bytes.contains(&b':') {
            return Err(ShapeError::DevicePath);
        }
        Ok(Self(raw.to_os_string()))
    }

    pub fn as_os_str(&self) -> &OsStr {
        &self.0
    }
}

/// Displays a path with control bytes as `\n`, `\r`, `\t` or `\xHH`, and bytes that are not
/// UTF-8 as `\xHH`. Backslashes stay literal, as in Mole's log grammar. This is the one escaping
/// rule for terminal output, `--json` and log rows (SAFE-22), so a name cannot forge a row.
pub fn escaped(raw: &OsStr) -> Escaped<'_> {
    Escaped(raw)
}

pub struct Escaped<'a>(&'a OsStr);

impl fmt::Display for Escaped<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for chunk in self.0.as_encoded_bytes().utf8_chunks() {
            for c in chunk.valid().chars() {
                match c {
                    '\n' => f.write_str("\\n")?,
                    '\r' => f.write_str("\\r")?,
                    '\t' => f.write_str("\\t")?,
                    c if c.is_control() => write!(f, "\\x{:02X}", c as u32)?,
                    c => f.write_char(c)?,
                }
            }
            for b in chunk.invalid() {
                write!(f, "\\x{b:02X}")?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Result<AbsPath, ShapeError> {
        AbsPath::parse(OsStr::new(s))
    }

    #[cfg(unix)]
    #[test]
    fn malformed_paths_are_refused_with_their_cause() {
        assert_eq!(parse(""), Err(ShapeError::Empty));
        assert_eq!(parse("relative/path"), Err(ShapeError::Relative));
        assert_eq!(parse("./x"), Err(ShapeError::Relative));
        assert_eq!(parse("/a/../b"), Err(ShapeError::ParentComponent));
        assert_eq!(parse("/a/.."), Err(ShapeError::ParentComponent));
        assert_eq!(parse("/a/bad\nname"), Err(ShapeError::ControlByte));
        assert_eq!(parse("/a/\x7f"), Err(ShapeError::ControlByte));
        assert_eq!(parse("/a/\0b"), Err(ShapeError::ControlByte));
    }

    #[cfg(unix)]
    #[test]
    fn a_name_with_two_dots_inside_is_valid() {
        let path = "/Users/a/Library/Application Support/Firefox/Profiles/x/name..files";
        assert_eq!(parse(path).unwrap().as_path(), Path::new(path));
    }

    #[cfg(unix)]
    #[test]
    fn equivalent_spellings_normalize_to_one() {
        let want = Path::new("/Users/me/Library/Caches");
        for raw in [
            "/Users//me/./Library/Caches/",
            "/Users/me/Library/Caches/.",
            "//Users/me/Library/Caches",
        ] {
            assert_eq!(parse(raw).unwrap().as_path(), want, "{raw}");
        }
        assert_eq!(parse("/").unwrap().as_path(), Path::new("/"));
    }

    #[cfg(windows)]
    #[test]
    fn windows_accepts_drive_paths_only() {
        assert!(parse(r"C:\Windows").is_ok());
        assert_eq!(
            parse(r"C:/Users//a/./x/").unwrap().as_path(),
            Path::new(r"C:\Users\a\x")
        );
        assert_eq!(parse(r"\\server\share\x"), Err(ShapeError::DevicePath));
        assert_eq!(parse(r"\\?\C:\Windows"), Err(ShapeError::DevicePath));
        assert_eq!(parse(r"\\.\PhysicalDrive0"), Err(ShapeError::DevicePath));
        assert_eq!(parse(r"C:\a\file.txt:stream"), Err(ShapeError::DevicePath));
        assert_eq!(parse(r"C:relative"), Err(ShapeError::Relative));
        assert_eq!(parse(r"\rooted"), Err(ShapeError::Relative));
        assert_eq!(parse(r"C:\a\..\b"), Err(ShapeError::ParentComponent));
    }

    #[test]
    fn a_name_is_one_plain_component() {
        let name = |s: &str| Name::parse(OsStr::new(s));
        assert!(name("cache.db").is_ok());
        assert!(name("name..files").is_ok());
        assert_eq!(name(""), Err(ShapeError::Empty));
        assert_eq!(name("."), Err(ShapeError::ParentComponent));
        assert_eq!(name(".."), Err(ShapeError::ParentComponent));
        assert_eq!(name("a/b"), Err(ShapeError::Relative));
        assert_eq!(name("a\tb"), Err(ShapeError::ControlByte));
    }

    #[cfg(unix)]
    #[test]
    fn join_with_a_name_never_yields_the_parent() {
        let root = parse("/Users/a").unwrap();
        assert!(Name::parse(OsStr::new("")).is_err());
        let child = root.join(&Name::parse(OsStr::new("x")).unwrap());
        assert_eq!(child.as_path(), Path::new("/Users/a/x"));
        assert_eq!(child.parent().unwrap(), root);
    }

    #[test]
    fn control_bytes_print_escaped() {
        let shown = escaped(OsStr::new("bad\nname\r\t\x1b[31m\x7f")).to_string();
        assert_eq!(shown, r"bad\nname\r\t\x1B[31m\x7F");
        assert_eq!(escaped(OsStr::new(r"C:\a\b")).to_string(), r"C:\a\b");
    }

    #[cfg(unix)]
    #[test]
    fn bytes_that_are_not_utf8_print_escaped() {
        use std::os::unix::ffi::OsStrExt as _;
        let raw = OsStr::from_bytes(b"/tmp/\xff\xfex");
        assert_eq!(escaped(raw).to_string(), r"/tmp/\xFF\xFEx");
    }
}
