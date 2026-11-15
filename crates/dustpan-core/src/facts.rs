use std::fmt;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallMethod {
    Homebrew,
    Cargo,
    Source,
    Manual,
}

impl InstallMethod {
    /// `exe` is the running binary's path and `cargo_bin` is the folder `cargo install` writes to.
    pub fn classify(exe: &Path, cargo_bin: Option<&Path>) -> Self {
        const BREW_PREFIXES: [&str; 3] = [
            "/opt/homebrew",
            "/usr/local/Homebrew",
            "/home/linuxbrew/.linuxbrew",
        ];
        if exe.components().any(|c| c.as_os_str() == "Cellar")
            || BREW_PREFIXES.iter().any(|p| exe.starts_with(p))
        {
            Self::Homebrew
        } else if cargo_bin.is_some_and(|dir| exe.starts_with(dir)) {
            Self::Cargo
        } else if exe.components().any(|c| c.as_os_str() == "target") {
            Self::Source
        } else {
            Self::Manual
        }
    }
}

impl fmt::Display for InstallMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Homebrew => "homebrew",
            Self::Cargo => "cargo",
            Self::Source => "source",
            Self::Manual => "manual",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OsFacts {
    pub name: String,
    pub version: String,
    pub arch: &'static str,
    pub install: InstallMethod,
}

impl fmt::Display for OsFacts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let os = format!("{} {}", self.name, self.version);
        write!(f, "{} · {} · {}", os.trim(), self.arch, self.install)
    }
}

/// Reads `NAME` and `VERSION_ID` from `/etc/os-release` text, falling back to `PRETTY_NAME`.
pub fn parse_os_release(text: &str) -> (String, String) {
    let field = |key: &str| {
        text.lines().find_map(|line| {
            let value = line.strip_prefix(key)?.strip_prefix('=')?;
            Some(value.trim().trim_matches(['"', '\'']).to_owned())
        })
    };
    let name = field("NAME").or_else(|| field("PRETTY_NAME"));
    (
        name.unwrap_or_else(|| "Linux".to_owned()),
        field("VERSION_ID").unwrap_or_default(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classify(exe: &str) -> InstallMethod {
        InstallMethod::classify(Path::new(exe), Some(Path::new("/home/u/.cargo/bin")))
    }

    #[test]
    fn install_method_by_location() {
        assert_eq!(
            classify("/opt/homebrew/bin/dustpan"),
            InstallMethod::Homebrew
        );
        assert_eq!(
            classify("/usr/local/Cellar/dustpan/0.1.0/bin/dustpan"),
            InstallMethod::Homebrew
        );
        assert_eq!(classify("/home/u/.cargo/bin/dustpan"), InstallMethod::Cargo);
        assert_eq!(
            classify("/home/u/src/dustpan/target/release/dustpan"),
            InstallMethod::Source
        );
        assert_eq!(classify("/usr/local/bin/dustpan"), InstallMethod::Manual);
    }

    #[test]
    fn facts_line_has_three_fields() {
        let facts = OsFacts {
            name: "macOS".into(),
            version: "15.6".into(),
            arch: "arm64",
            install: InstallMethod::Cargo,
        };
        assert_eq!(facts.to_string(), "macOS 15.6 · arm64 · cargo");
    }

    #[test]
    fn os_release_fields() {
        let ubuntu = "PRETTY_NAME=\"Ubuntu 24.04.1 LTS\"\nNAME=\"Ubuntu\"\nVERSION_ID=\"24.04\"\n";
        assert_eq!(parse_os_release(ubuntu), ("Ubuntu".into(), "24.04".into()));
        let arch = "NAME='Arch Linux'\nID=arch\n";
        assert_eq!(parse_os_release(arch), ("Arch Linux".into(), String::new()));
        assert_eq!(parse_os_release(""), ("Linux".into(), String::new()));
    }

    #[test]
    fn facts_line_skips_an_empty_version() {
        let facts = OsFacts {
            name: "Arch Linux".into(),
            version: String::new(),
            arch: "x86_64",
            install: InstallMethod::Manual,
        };
        assert_eq!(facts.to_string(), "Arch Linux · x86_64 · manual");
    }
}
