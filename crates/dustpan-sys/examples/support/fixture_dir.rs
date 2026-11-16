//! A fixture folder that an example or a bench makes for itself and removes when it is done.
//! It is made exclusively under a fresh random name, so nothing planted at a name someone could
//! guess, such as a link at `dustpan-<name>-<pid>`, is ever used. It is removed only while it is
//! still the folder this run made: not a link, and the same file id as when it was made.
use dustpan_core::identity::Node;
use dustpan_core::path::AbsPath;
use dustpan_core::probe::{Probe, Seen};
use dustpan_sys::LiveProbe;
use std::path::Path;

pub struct FixtureDir {
    path: AbsPath,
    made: Node,
}

impl FixtureDir {
    /// Under the temporary folder, not the repository, which an editor or a terminal may watch
    /// and so hold open, and the guard would rightly keep what is held. `.noindex` keeps
    /// Spotlight out.
    pub fn new(prefix: &str) -> Self {
        // The guard refuses a path with a linked ancestor (`/var` on macOS), so the temporary
        // folder is resolved before anything is made in it. The new folder is never resolved.
        let parent = std::env::temp_dir()
            .canonicalize()
            .expect("the temporary folder resolves");
        let made = tempfile::Builder::new()
            .prefix(prefix)
            .suffix(".noindex")
            .tempdir_in(parent)
            .expect("fixture folder")
            .keep();
        let path = AbsPath::parse(made.as_os_str()).expect("fixture folder path");
        let Seen::Found(made) = LiveProbe::new().node(&path) else {
            panic!("no {path}")
        };
        Self { path, made }
    }

    pub fn path(&self) -> &Path {
        self.path.as_path()
    }
}

impl Drop for FixtureDir {
    fn drop(&mut self) {
        if LiveProbe::new().node(&self.path) != Seen::Found(self.made) {
            eprintln!(
                "kept {}: it is no longer the folder this run made",
                self.path
            );
            return;
        }
        #[expect(
            clippy::disallowed_methods,
            reason = "removes only the folder this run made, checked just above"
        )]
        let _ = std::fs::remove_dir_all(self.path.as_path());
    }
}
