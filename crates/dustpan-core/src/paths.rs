//! Folder rules. Each takes the OS and an env lookup as arguments, so tests need no real environment.
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Os {
    MacOs,
    Linux,
    Windows,
}

impl Os {
    pub const fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else if cfg!(windows) {
            Self::Windows
        } else {
            Self::Linux
        }
    }
}

pub type Env<'a> = &'a dyn Fn(&str) -> Option<String>;

fn var(env: Env, key: &str) -> Option<PathBuf> {
    env(key).filter(|v| !v.is_empty()).map(PathBuf::from)
}

pub fn home_dir(os: Os, env: Env) -> Option<PathBuf> {
    var(
        env,
        if os == Os::Windows {
            "USERPROFILE"
        } else {
            "HOME"
        },
    )
}

pub fn log_dir(os: Os, env: Env) -> Option<PathBuf> {
    match os {
        Os::MacOs => Some(home_dir(os, env)?.join("Library/Logs/dustpan")),
        Os::Linux => {
            let state = var(env, "XDG_STATE_HOME")
                .filter(|p| p.starts_with("/"))
                .or_else(|| Some(home_dir(os, env)?.join(".local/state")))?;
            Some(state.join("dustpan"))
        }
        Os::Windows => Some(var(env, "LOCALAPPDATA")?.join("dustpan").join("logs")),
    }
}

pub fn cargo_bin_dir(os: Os, env: Env) -> Option<PathBuf> {
    let cargo_home = var(env, "CARGO_HOME").or_else(|| Some(home_dir(os, env)?.join(".cargo")))?;
    Some(cargo_home.join("bin"))
}

/// Replaces the home folder prefix with `~` so a log names no user.
pub fn redact_home(text: &str, home: Option<&Path>) -> String {
    match home.map(|h| h.to_string_lossy()) {
        Some(h) if h.len() > 1 => text.replace(h.as_ref(), "~"),
        _ => text.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| {
            pairs
                .iter()
                .find(|(key, _)| *key == k)
                .map(|(_, v)| (*v).to_owned())
        }
    }

    #[test]
    fn macos_logs_live_under_library() {
        let e = env(&[("HOME", "/Users/a")]);
        assert_eq!(
            log_dir(Os::MacOs, &e),
            Some(PathBuf::from("/Users/a/Library/Logs/dustpan"))
        );
    }

    #[test]
    fn linux_prefers_xdg_state_home_and_ignores_relative_values() {
        let set = env(&[("HOME", "/home/a"), ("XDG_STATE_HOME", "/var/s")]);
        assert_eq!(
            log_dir(Os::Linux, &set),
            Some(PathBuf::from("/var/s/dustpan"))
        );
        let relative = env(&[("HOME", "/home/a"), ("XDG_STATE_HOME", "s")]);
        let unset = env(&[("HOME", "/home/a")]);
        let expected = Some(PathBuf::from("/home/a/.local/state/dustpan"));
        assert_eq!(log_dir(Os::Linux, &relative), expected);
        assert_eq!(log_dir(Os::Linux, &unset), expected);
    }

    #[test]
    fn windows_logs_live_under_local_app_data() {
        let e = env(&[("LOCALAPPDATA", r"C:\Users\a\AppData\Local")]);
        assert_eq!(
            log_dir(Os::Windows, &e),
            Some(
                PathBuf::from(r"C:\Users\a\AppData\Local")
                    .join("dustpan")
                    .join("logs")
            )
        );
    }

    #[test]
    fn no_home_means_no_log_dir() {
        assert_eq!(log_dir(Os::MacOs, &env(&[])), None);
    }

    #[test]
    fn cargo_bin_follows_cargo_home_then_home() {
        let custom = env(&[("CARGO_HOME", "/c"), ("HOME", "/h")]);
        let default = env(&[("HOME", "/h")]);
        assert_eq!(
            cargo_bin_dir(Os::Linux, &custom),
            Some(PathBuf::from("/c/bin"))
        );
        assert_eq!(
            cargo_bin_dir(Os::Linux, &default),
            Some(PathBuf::from("/h/.cargo/bin"))
        );
    }

    #[test]
    fn redaction_swaps_home_for_tilde() {
        let home = Path::new("/Users/a");
        assert_eq!(
            redact_home("/Users/a/x --y=/Users/a", Some(home)),
            "~/x --y=~"
        );
        assert_eq!(redact_home("/Users/a/x", None), "/Users/a/x");
        assert_eq!(redact_home("/x", Some(Path::new("/"))), "/x");
    }
}
