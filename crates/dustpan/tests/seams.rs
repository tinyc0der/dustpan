//! SAFE-17: a test seam never steers a real run. The fake probe is compiled only with core's
//! `testkit` feature, so no shipped crate may name that feature or the fake.
use std::path::{Path, PathBuf};

const SEAMS: [&str; 3] = ["testkit", "FakeProbe", "probe::fake"];

fn files_under(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_dir() {
        for entry in std::fs::read_dir(path).unwrap().flatten() {
            files_under(&entry.path(), out);
        }
    } else {
        out.push(path.to_path_buf());
    }
}

/// Every line in `path`, a file or a folder, that names a test seam.
fn seam_lines(path: &Path) -> Vec<String> {
    let mut files = Vec::new();
    files_under(path, &mut files);
    let mut found = Vec::new();
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap();
        for (number, line) in text.lines().enumerate() {
            if SEAMS.iter().any(|seam| line.contains(seam)) {
                found.push(format!(
                    "{}:{}: {}",
                    file.display(),
                    number + 1,
                    line.trim()
                ));
            }
        }
    }
    found
}

fn crates() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

#[test]
fn no_shipped_crate_names_the_fake_probe_or_its_feature() {
    let shipped = [
        "dustpan/src",
        "dustpan/Cargo.toml",
        "dustpan-sys/src",
        "dustpan-sys/Cargo.toml",
    ];
    let found: Vec<String> = shipped
        .iter()
        .flat_map(|path| seam_lines(&crates().join(path)))
        .collect();
    assert!(found.is_empty(), "{}", found.join("\n"));
}

#[test]
fn the_search_finds_the_seam_where_it_is_allowed() {
    assert!(!seam_lines(&crates().join("dustpan-core/Cargo.toml")).is_empty());
    assert!(!seam_lines(&crates().join("dustpan-core/src/probe/fake.rs")).is_empty());
}
