//! `cargo bench -p dustpan-sys --bench sink_delete`: the Delete sink against `/bin/rm -rf` on
//! two identical 100,000-file trees (1,000 folders of 100 one-byte files) in the bench's own
//! folder under the target directory. Ten rounds, alternating which runs first. It takes no path
//! argument, so it cannot become the test-only delete command Appendix B of the M0 checklist
//! rejects.
#![cfg_attr(
    unix,
    expect(
        clippy::disallowed_methods,
        reason = "the bench builds its own fixture trees; the bans guard shipped code"
    )
)]

#[cfg(not(unix))]
fn main() {
    println!("sink bench: the baseline is /bin/rm -rf; run it on macOS or Linux");
}

#[cfg(unix)]
fn main() -> std::process::ExitCode {
    unix::run()
}

#[cfg(unix)]
mod unix {
    use std::path::{Path, PathBuf};
    use std::process::{Command, ExitCode, Stdio};
    use std::time::{Duration, Instant};

    const FOLDERS: usize = 1_000;
    const FILES: usize = 100;
    const ROUNDS: usize = 10;

    fn build(tree: &Path) {
        for folder in 0..FOLDERS {
            let dir = tree.join(format!("d{folder:04}"));
            std::fs::create_dir_all(&dir).expect("fixture folder");
            for file in 0..FILES {
                std::fs::write(dir.join(format!("f{file:03}")), b"x").expect("fixture file");
            }
        }
    }

    fn rm_rf(tree: &Path) -> Duration {
        let start = Instant::now();
        let status = Command::new("/bin/rm")
            .arg("-rf")
            .arg(tree)
            .stdin(Stdio::null())
            .status()
            .expect("/bin/rm runs");
        let took = start.elapsed();
        assert!(status.success(), "rm -rf failed");
        assert!(!tree.exists(), "rm -rf left the tree");
        took
    }

    fn median(mut runs: Vec<Duration>) -> Duration {
        runs.sort();
        runs[runs.len() / 2]
    }

    pub(super) fn run() -> ExitCode {
        let root: PathBuf = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("sink-bench-{}", std::process::id()));
        let mut rm = Vec::with_capacity(ROUNDS);
        for round in 0..ROUNDS {
            let tree = root.join(format!("round{round}/rm"));
            build(&tree);
            let took = rm_rf(&tree);
            println!("round {round}: rm -rf {:.3} s", took.as_secs_f64());
            rm.push(took);
        }
        let _ = std::fs::remove_dir_all(&root);
        println!(
            "rm -rf median {:.3} s over {ROUNDS} rounds of {} files",
            median(rm).as_secs_f64(),
            FOLDERS * FILES
        );
        ExitCode::SUCCESS
    }
}
