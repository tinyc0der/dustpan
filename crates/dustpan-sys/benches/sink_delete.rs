//! `cargo bench -p dustpan-sys --bench sink_delete`: the Delete sink against `/bin/rm -rf` on
//! two identical 100,000-file trees (1,000 folders of 100 one-byte files) in the bench's own
//! temporary folder. Ten rounds, alternating which runs first. Fails when the
//! sink's median exceeds twice the `rm -rf` median (M0-5 perf box). It takes no path argument,
//! so it cannot become the test-only delete command Appendix B of the M0 checklist rejects.
//!
//! The sink's number is the whole item as `clean` pays it: `Recheck::approve` (every guard
//! layer, identity, one process table and one snapshot of open files, and the survey that lists
//! and judges all 101,000 entries) plus the delete walk and the log row.
#[cfg(unix)]
#[path = "../examples/support/fixture_dir.rs"]
mod fixture_dir;

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
    use crate::fixture_dir::FixtureDir;
    use dustpan_core::candidate::{GroupId, Rule, RuleId, Sink};
    use dustpan_core::guard::{Guard, GuardInputs, Homes, Lease};
    use dustpan_core::oplog::{Command, OpLog, RunId};
    use dustpan_core::path::AbsPath;
    use dustpan_core::paths::Os;
    use dustpan_core::probe::{Probe, Seen};
    use dustpan_core::sink::{Cancel, Executor, Mode, Outcome, Services};
    use dustpan_core::size::Size;
    use dustpan_core::time::Clock;
    use dustpan_core::whitelist::Whitelist;
    use dustpan_sys::{FileStore, LiveDisk, LiveProbe, SystemClock};
    use std::path::Path;
    use std::process::{Command as Process, ExitCode, Stdio};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    const FOLDERS: usize = 1_000;
    const FILES: usize = 100;
    const ROUNDS: usize = 10;
    const BUDGET: f64 = 2.0;

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
        #[expect(
            clippy::disallowed_methods,
            reason = "`/bin/rm -rf` is the baseline the sink is measured against"
        )]
        let status = Process::new("/bin/rm")
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

    /// The tree as one `clean` item: discovered, then rechecked, surveyed and removed by the
    /// executor in apply mode. Only the executor is timed.
    fn sink(home: &Path, tree: &Path) -> Duration {
        let probe = LiveProbe::new();
        let env = |_: &str| None;
        let home = AbsPath::parse(home.as_os_str()).expect("bench home");
        let inputs = GuardInputs {
            os: Os::current(),
            homes: Homes::new(home.clone(), None),
            env: &env,
            whitelist: Whitelist::defaults(Os::current(), &Homes::new(home.clone(), None)),
        };
        let guard = Guard::new(inputs, &probe).expect("guard");
        let rule = Arc::new(Rule {
            id: RuleId("bench.cache".into()),
            group: GroupId("bench".into()),
            owners: Box::new([]),
            sink: Sink::Delete,
            default_selected: true,
        });
        let path = AbsPath::parse(tree.as_os_str()).expect("bench tree");
        let Seen::Found(node) = probe.node(&path) else {
            panic!("no bench tree")
        };
        let candidate = guard
            .pass(&probe)
            .candidate(path, node, &rule)
            .expect("candidate");
        let clock = SystemClock::new();
        let cancel = Cancel::default();
        let mut log = OpLog::new(FileStore::new(home.as_path().join("logs")), |w: &_| {
            panic!("log: {w}")
        });
        let disk = LiveDisk::new();
        let services = Services {
            guard: &guard,
            probe: &probe,
            clock: &clock,
            lease: Lease::BATCH,
            journal: &mut log,
            cancel: &cancel,
        };
        let start = Instant::now();
        let id = RunId::new(clock.wall(), std::process::id(), 0);
        let mut executor =
            Executor::start(Mode::Apply(&disk), Command::named("bench"), id, services);
        let (_, outcome) = executor.run(candidate);
        executor.finish();
        let took = start.elapsed();
        // A run that refused or did nothing must not pass as a fast one.
        assert!(
            matches!(outcome, Outcome::Removed(Size::Known(n)) if n > 0),
            "{outcome:?}"
        );
        assert!(!tree.exists(), "the sink left the tree");
        took
    }

    fn median(mut runs: Vec<Duration>) -> Duration {
        runs.sort();
        runs[runs.len() / 2]
    }

    fn secs(d: Duration) -> f64 {
        d.as_secs_f64()
    }

    pub(super) fn run() -> ExitCode {
        let fixture = FixtureDir::new("dustpan-sink-bench-");
        let root = fixture.path();
        std::fs::create_dir_all(root.join("Library/Caches")).expect("bench root");
        let (mut rm, mut ours) = (Vec::new(), Vec::new());
        for round in 0..ROUNDS {
            let a = root.join(format!("Library/Caches/rm-{round}"));
            let b = root.join(format!("Library/Caches/sink-{round}"));
            build(&a);
            build(&b);
            let (took_rm, took_sink) = if round % 2 == 0 {
                (rm_rf(&a), sink(root, &b))
            } else {
                let took_sink = sink(root, &b);
                (rm_rf(&a), took_sink)
            };
            println!(
                "round {round}: rm -rf {:.3} s, sink {:.3} s",
                secs(took_rm),
                secs(took_sink)
            );
            rm.push(took_rm);
            ours.push(took_sink);
        }
        let (rm, ours) = (median(rm), median(ours));
        let ratio = secs(ours) / secs(rm);
        println!(
            "rm -rf median {:.3} s, sink median {:.3} s, ratio {ratio:.2} (budget {BUDGET:.1}), \
             {ROUNDS} rounds of {} files",
            secs(rm),
            secs(ours),
            FOLDERS * FILES
        );
        if ratio > BUDGET {
            println!("FAIL: the sink is more than {BUDGET} times rm -rf");
            return ExitCode::FAILURE;
        }
        ExitCode::SUCCESS
    }
}
