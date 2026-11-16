use crate::error::{Error, Result, io_err};
use filetime::FileTime;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::TempDir;

const SECS_PER_DAY: i64 = 86_400;

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Manifest {
    pub dir: Vec<DirSpec>,
    pub file: Vec<FileSpec>,
    pub symlink: Vec<LinkSpec>,
    pub zip: Vec<ZipSpec>,
    pub age: Vec<AgeSpec>,
    pub mole: MoleConfig,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DirSpec {
    pub path: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileSpec {
    pub path: String,
    pub size: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinkSpec {
    pub path: String,
    pub target: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ZipSpec {
    pub path: String,
    pub entries: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgeSpec {
    pub path: String,
    pub days: i64,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MoleConfig {
    pub whitelist: Vec<String>,
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Manifest> {
        let manifest: Manifest =
            toml::from_str(text).map_err(|e| Error::Manifest(e.to_string()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<()> {
        let paths = self
            .dir
            .iter()
            .map(|d| &d.path)
            .chain(self.file.iter().map(|f| &f.path))
            .chain(self.symlink.iter().map(|l| &l.path))
            .chain(self.zip.iter().map(|z| &z.path))
            .chain(self.age.iter().map(|a| &a.path))
            .chain(self.symlink.iter().map(|l| &l.target))
            .chain(self.mole.whitelist.iter());
        for p in paths {
            relative(p)?;
        }
        let placed = self
            .dir
            .iter()
            .map(|d| (&d.path, false))
            .chain(self.file.iter().map(|f| (&f.path, false)))
            .chain(self.zip.iter().map(|z| (&z.path, false)))
            .chain(self.symlink.iter().map(|l| (&l.path, true)));
        for (p, is_link) in placed {
            if reserved(p, is_link) {
                return Err(Error::Manifest(format!(
                    "path {p:?} is inside {MOLE_CONFIG}, which the harness owns: a manifest must not write Mole's config"
                )));
            }
        }
        for name in self.zip.iter().flat_map(|z| &z.entries) {
            if has_control(name) {
                return Err(Error::Manifest(format!(
                    "zip entry name {name:?} holds a control character"
                )));
            }
        }
        Ok(())
    }
}

const MOLE_CONFIG: &str = "home/.config/mole";

fn has_control(s: &str) -> bool {
    s.chars().any(char::is_control)
}

/// APFS is case-insensitive by default, so compare folded names. A link at
/// `home` or `home/.config` would redirect the config folder as well.
fn reserved(path: &str, is_link: bool) -> bool {
    let fold = |s: &str| -> Vec<String> {
        Path::new(s)
            .components()
            .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
            .collect()
    };
    let (path, config) = (fold(path), fold(MOLE_CONFIG));
    path.starts_with(&config) || (is_link && config.starts_with(&path))
}

fn relative(p: &str) -> Result<&Path> {
    let path = Path::new(p);
    let ok = path.components().all(|c| matches!(c, Component::Normal(_)));
    if p.is_empty() || !ok || has_control(p) {
        return Err(Error::Manifest(format!(
            "path {p:?} must be relative to the fixture root with no `..`, absolute parts or control characters"
        )));
    }
    Ok(path)
}

pub struct Fixture {
    dir: TempDir,
    root: PathBuf,
}

impl Fixture {
    pub fn build(manifest: &Manifest) -> Result<Fixture> {
        let dir = tempfile::Builder::new()
            .prefix("dustpan-parity.")
            .tempdir()
            .map_err(io_err("create fixture folder"))?;
        let root = dir
            .path()
            .canonicalize()
            .map_err(io_err("resolve fixture folder"))?;
        assert_under_temp(&root)?;
        let fixture = Fixture { dir, root };
        fixture.populate(manifest)?;
        Ok(fixture)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn home(&self) -> PathBuf {
        self.root.join("home")
    }

    pub fn tmp(&self) -> PathBuf {
        self.root.join("tmp")
    }

    pub fn keep(self) -> PathBuf {
        let _ = self.dir.keep();
        self.root
    }

    fn populate(&self, m: &Manifest) -> Result<()> {
        for sub in ["home", "victim", "tmp"] {
            mkdir(&self.root.join(sub))?;
        }
        for d in &m.dir {
            mkdir(&self.root.join(relative(&d.path)?))?;
        }
        for f in &m.file {
            write_file(
                &self.root.join(relative(&f.path)?),
                &vec![0u8; f.size as usize],
            )?;
        }
        for z in &m.zip {
            write_file(&self.root.join(relative(&z.path)?), &stored_zip(&z.entries))?;
        }
        for l in &m.symlink {
            let at = self.root.join(relative(&l.path)?);
            if let Some(parent) = at.parent() {
                mkdir(parent)?;
            }
            symlink(self.root.join(relative(&l.target)?), &at)
                .map_err(io_err(format!("symlink {}", l.path)))?;
        }
        if !m.mole.whitelist.is_empty() {
            let mut lines = String::new();
            for entry in &m.mole.whitelist {
                lines.push_str(&self.root.join(relative(entry)?).to_string_lossy());
                lines.push('\n');
            }
            write_file(
                &self.home().join(".config/mole/whitelist"),
                lines.as_bytes(),
            )?;
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        for a in &m.age {
            let mtime = FileTime::from_unix_time(now - a.days * SECS_PER_DAY, 0);
            set_tree_mtime(&self.root.join(relative(&a.path)?), mtime)?;
        }
        Ok(())
    }
}

/// Folders a throwaway fixture may live in. `$TMPDIR` is deliberately not
/// consulted: pointing it at a home folder must not make the home acceptable.
const TEMP_ROOTS: &[&str] = &["/tmp", "/var/tmp", "/private/tmp", "/private/var/folders"];

pub fn assert_under_temp(path: &Path) -> Result<()> {
    let path = path
        .canonicalize()
        .map_err(io_err("resolve fixture path"))?;
    let under_temp = TEMP_ROOTS
        .iter()
        .filter_map(|r| Path::new(r).canonicalize().ok())
        .any(|root| path.starts_with(root));
    if under_temp {
        Ok(())
    } else {
        Err(Error::FixtureOutsideTemp(path.display().to_string()))
    }
}

fn mkdir(p: &Path) -> Result<()> {
    fs::create_dir_all(p).map_err(io_err(format!("create {}", p.display())))
}

fn write_file(p: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = p.parent() {
        mkdir(parent)?;
    }
    fs::write(p, bytes).map_err(io_err(format!("write {}", p.display())))
}

fn set_tree_mtime(p: &Path, mtime: FileTime) -> Result<()> {
    let meta = fs::symlink_metadata(p).map_err(io_err(format!("stat {}", p.display())))?;
    if meta.is_dir() {
        let entries = fs::read_dir(p).map_err(io_err(format!("read {}", p.display())))?;
        for entry in entries {
            set_tree_mtime(&entry.map_err(io_err("read entry"))?.path(), mtime)?;
        }
    }
    filetime::set_symlink_file_times(p, mtime, mtime)
        .map_err(io_err(format!("set mtime {}", p.display())))
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn stored_zip(entries: &[String]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for name in entries {
        let data = b"fixture\n";
        let crc = crc32(data);
        let offset = out.len() as u32;
        let name = name.as_bytes();
        let mut local = Vec::new();
        local.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        local.extend_from_slice(&[20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        local.extend_from_slice(&crc.to_le_bytes());
        local.extend_from_slice(&(data.len() as u32).to_le_bytes());
        local.extend_from_slice(&(data.len() as u32).to_le_bytes());
        local.extend_from_slice(&(name.len() as u16).to_le_bytes());
        local.extend_from_slice(&0u16.to_le_bytes());
        local.extend_from_slice(name);
        local.extend_from_slice(data);
        out.extend_from_slice(&local);

        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        central.extend_from_slice(&[20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&(data.len() as u32).to_le_bytes());
        central.extend_from_slice(&(data.len() as u32).to_le_bytes());
        central.extend_from_slice(&(name.len() as u16).to_le_bytes());
        central.extend_from_slice(&[0; 12]);
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name);
    }
    let central_offset = out.len() as u32;
    let count = entries.len() as u16;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&[0, 0, 0, 0]);
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&central_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Dir,
    File { size: u64, mtime: i64 },
    Link { target: PathBuf, mtime: i64 },
}

pub type Snapshot = BTreeMap<PathBuf, Kind>;

pub fn snapshot(root: &Path) -> Result<Snapshot> {
    let mut out = Snapshot::new();
    walk(root, root, &mut out)?;
    Ok(out)
}

fn walk(root: &Path, p: &Path, out: &mut Snapshot) -> Result<()> {
    let rel = p.strip_prefix(root).unwrap_or(p).to_path_buf();
    let meta = fs::symlink_metadata(p).map_err(io_err(format!("stat {}", p.display())))?;
    let mtime = FileTime::from_last_modification_time(&meta).unix_seconds();
    if meta.is_dir() {
        out.insert(rel, Kind::Dir);
        let entries = fs::read_dir(p).map_err(io_err(format!("read {}", p.display())))?;
        for entry in entries {
            walk(root, &entry.map_err(io_err("read entry"))?.path(), out)?;
        }
    } else if meta.is_symlink() {
        let target = fs::read_link(p).map_err(io_err(format!("readlink {}", p.display())))?;
        out.insert(rel, Kind::Link { target, mtime });
    } else {
        out.insert(
            rel,
            Kind::File {
                size: meta.len(),
                mtime,
            },
        );
    }
    Ok(())
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Drift {
    pub modified: Vec<PathBuf>,
    pub removed: Vec<PathBuf>,
    pub added: Vec<PathBuf>,
    /// Files that host tools Mole calls (brew, npm, python3) create in the home.
    pub host_tool_files: usize,
}

impl Drift {
    pub fn is_clean(&self) -> bool {
        self.modified.is_empty() && self.removed.is_empty() && self.added.is_empty()
    }
}

const HOST_TOOL_DIRS: &[&str] = &[
    "home/.npm",
    "home/Library/Caches/Homebrew",
    "home/Library/Caches/com.apple.python",
];

const IGNORED: &[&str] = &[
    "home/.config/mole",
    "home/Library/Logs/mole",
    "home/.cache/mole",
    "tmp",
];

fn is_ignored(rel: &Path) -> bool {
    IGNORED.iter().any(|s| rel.starts_with(s))
}

pub fn drift(before: &Snapshot, after: &Snapshot) -> Drift {
    let mut d = Drift::default();
    for (path, was) in before {
        if is_ignored(path) {
            continue;
        }
        match after.get(path) {
            None => d.removed.push(path.clone()),
            Some(Kind::Dir) if *was == Kind::Dir => {}
            Some(now) if now == was => {}
            Some(_) => d.modified.push(path.clone()),
        }
    }
    for (path, kind) in after {
        let holds_ignored =
            *kind == Kind::Dir && IGNORED.iter().any(|s| Path::new(s).starts_with(path));
        if before.contains_key(path) || is_ignored(path) || holds_ignored {
            continue;
        }
        if HOST_TOOL_DIRS.iter().any(|s| path.starts_with(s)) {
            d.host_tool_files += 1;
        } else {
            d.added.push(path.clone());
        }
    }
    d
}
