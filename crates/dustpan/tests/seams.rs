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

/// SAFE-01: the files allowed to lift the `clippy.toml` removal bans. A lifted
/// `clippy::disallowed_methods` anywhere else in a shipped crate fails this test, so a second
/// removal path cannot appear without a change to this list in review.
const SINK_FILES: [&str; 6] = [
    "dustpan-sys/src/unix/delete.rs",
    "dustpan-sys/src/macos/trash.rs",
    "dustpan-sys/src/linux/trash.rs",
    "dustpan-sys/src/windows/delete.rs",
    "dustpan-sys/src/windows/trash.rs",
    // Rotation of dustpan's own log, with a `SAFE:` note beside the call.
    "dustpan-sys/src/oplog.rs",
];

#[test]
fn only_the_sink_files_lift_the_removal_bans() {
    let mut files = Vec::new();
    for root in ["dustpan/src", "dustpan-sys/src", "dustpan-core/src"] {
        files_under(&crates().join(root), &mut files);
    }
    let lifted: Vec<String> = files
        .iter()
        .filter(|file| {
            std::fs::read_to_string(file)
                .unwrap()
                .contains("clippy::disallowed_methods")
        })
        .map(|file| file.to_string_lossy().replace('\\', "/"))
        .filter(|file| !SINK_FILES.iter().any(|allowed| file.ends_with(allowed)))
        .collect();
    assert!(lifted.is_empty(), "removal outside the sink: {lifted:?}");
}
