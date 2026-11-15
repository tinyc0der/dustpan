//! `cargo bench -p dustpan-core --bench guard`: guard evaluations per second over 100,000
//! fixture paths. Fails below 200,000 per second (M0-3 perf box).
//!
//! The fixture is the home `tests/fixtures/mkhome.sh` builds, plus 100,000 cache entries, under
//! the target folder. It is built once and kept. It is walked once, outside timing, to collect
//! each entry's `Node` the way a scanner's directory read does. Three numbers are printed so
//! each can be explained:
//! - `scan` (the gated number): `Pass::candidate` with `LiveProbe` and walker nodes, so it counts
//!   core CPU plus the ancestor `lstat`s a scan really adds
//! - `explain`: `Pass::evaluate` with `LiveProbe`, one leaf `lstat` each
//! - `cpu`: `Pass::candidate` with `FakeProbe`, core CPU alone
//!
//! Each line also prints how many paths came back eligible, so a run that judged nothing, or
//! refused everything early, cannot pass as a fast one.
#![cfg_attr(
    unix,
    expect(
        clippy::disallowed_methods,
        reason = "the bench builds and walks a real fixture tree; the bans guard core's library code"
    )
)]

#[cfg(not(unix))]
fn main() {
    println!("guard bench: the fixture script needs a Unix shell; run it on macOS or Linux");
}

#[cfg(unix)]
fn main() -> std::process::ExitCode {
    unix::run()
}

#[cfg(unix)]
mod unix {
    use dustpan_core::candidate::{GroupId, Rule, RuleId, Sink};
    use dustpan_core::guard::{Guard, GuardInputs, Verdict};
    use dustpan_core::identity::Node;
    use dustpan_core::path::AbsPath;
    use dustpan_core::paths::Os;
    use dustpan_core::probe::fake::FakeProbe;
    use dustpan_core::probe::{Handles, Probe, ProcessTable, Seen};
    use dustpan_sys::LiveProbe;
    use std::hint::black_box;
    use std::path::{Path, PathBuf};
    use std::process::ExitCode;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    const PATHS: usize = 100_000;
    const ROUNDS: usize = 5;
    const BUDGET_PER_SECOND: f64 = 200_000.0;
    const APPS: usize = 100;
    const FOLDERS: usize = 20;
    const FILES: usize = 50;

    /// Counts the `lstat`s a shape costs, so a regression is traced to syscalls or to policy.
    struct Counting<'a> {
        inner: &'a dyn Probe,
        nodes: AtomicUsize,
    }

    impl Probe for Counting<'_> {
        fn node(&self, path: &AbsPath) -> Seen<Node> {
            self.nodes.fetch_add(1, Ordering::Relaxed);
            self.inner.node(path)
        }

        fn link_target(&self, link: &AbsPath) -> Seen<PathBuf> {
            self.inner.link_target(link)
        }

        fn processes(&self) -> Seen<ProcessTable> {
            self.inner.processes()
        }

        fn in_use(&self, files: &[AbsPath]) -> Handles {
            self.inner.in_use(files)
        }
    }

    fn fixture() -> (PathBuf, bool) {
        let home = Path::new(env!("CARGO_TARGET_TMPDIR")).join("guard-bench-home");
        let marker = home.join(format!(".bench-{PATHS}"));
        if marker.exists() {
            return (home, true);
        }
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/mkhome.sh");
        let built = std::process::Command::new("bash")
            .arg(script)
            .arg(&home)
            .status()
            .expect("bash runs the fixture script");
        assert!(built.success(), "mkhome.sh failed");
        for app in 0..APPS {
            for folder in 0..FOLDERS {
                let dir = home.join(format!(
                    "Library/Caches/com.example.bench{app:03}/Cache/{folder:02}"
                ));
                std::fs::create_dir_all(&dir).expect("fixture folder");
                for file in 0..FILES {
                    std::fs::write(dir.join(format!("blob-{file:04}.bin")), b"x")
                        .expect("fixture file");
                }
            }
        }
        std::fs::write(marker, b"").expect("fixture marker");
        (home, false)
    }

    /// Depth-first, sorted, the order a scanner feeds a pass.
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let mut entries: Vec<_> = std::fs::read_dir(dir)
            .expect("fixture is readable")
            .filter_map(Result::ok)
            .collect();
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let path = entry.path();
            let is_dir = entry.file_type().is_ok_and(|t| t.is_dir());
            out.push(path.clone());
            if is_dir {
                walk(&path, out);
            }
        }
    }

    fn nodes(probe: &dyn Probe, paths: &[AbsPath]) -> Vec<Node> {
        paths
            .iter()
            .map(|path| match probe.node(path) {
                Seen::Found(node) => node,
                other => panic!("fixture {path} is {other:?}"),
            })
            .collect()
    }

    struct Shape<'a> {
        name: &'static str,
        guard: &'a Guard,
        probe: Counting<'a>,
        /// `Some` feeds walker nodes to `Pass::candidate`; `None` calls `Pass::evaluate`.
        nodes: Option<&'a [Node]>,
        times: Vec<Duration>,
        eligible: usize,
    }

    impl Shape<'_> {
        fn round(&mut self, paths: &[AbsPath], rule: &Arc<Rule>) {
            // The scanner owns each path it hands over, so the copies are made before timing.
            let owned: Vec<AbsPath> = paths.to_vec();
            self.probe.nodes.store(0, Ordering::Relaxed);
            let mut eligible = 0;
            let start = Instant::now();
            let mut pass = self.guard.pass(&self.probe);
            for (i, path) in owned.into_iter().enumerate() {
                let verdict = match self.nodes {
                    Some(nodes) => match pass.candidate(path, nodes[i], rule) {
                        Ok(candidate) => candidate.verdict().clone(),
                        Err(judgment) => judgment.verdict().clone(),
                    },
                    None => pass.evaluate(path).verdict().clone(),
                };
                eligible += usize::from(black_box(verdict) == Verdict::Eligible);
            }
            self.times.push(start.elapsed());
            self.eligible = eligible;
        }

        /// Prints the line and returns the median rate.
        fn report(&mut self, paths: usize) -> f64 {
            self.times.sort();
            let median = self.times[self.times.len() / 2];
            let per_second = paths as f64 / median.as_secs_f64();
            println!(
                "{:>8}: {:>10.0} paths/s  median {:6.1} ms of {} rounds (min {:.1}, max {:.1})  \
                 {:.3} lstat per path  {} of {} eligible",
                self.name,
                per_second,
                median.as_secs_f64() * 1e3,
                self.times.len(),
                self.times[0].as_secs_f64() * 1e3,
                self.times[self.times.len() - 1].as_secs_f64() * 1e3,
                self.probe.nodes.load(Ordering::Relaxed) as f64 / paths as f64,
                self.eligible,
                paths,
            );
            per_second
        }
    }

    pub fn run() -> ExitCode {
        let (home, cached) = fixture();
        let home = home.canonicalize().expect("fixture home resolves");
        let mut found = Vec::new();
        walk(&home.join("Library/Caches"), &mut found);
        found.truncate(PATHS);
        assert_eq!(
            found.len(),
            PATHS,
            "the fixture holds fewer paths than the bench needs"
        );
        let paths: Vec<AbsPath> = found
            .iter()
            .map(|p| AbsPath::parse(p.as_os_str()).expect("fixture paths are absolute"))
            .collect();
        let home = AbsPath::parse(home.as_os_str()).expect("fixture home is absolute");

        let env = |_: &str| None;
        let inputs = || GuardInputs {
            os: Os::current(),
            home: home.clone(),
            env: &env,
        };
        let live = LiveProbe::new();
        let live_guard = Guard::new(inputs(), &live).expect("the policy loads on this host");
        let live_nodes = nodes(&live, &paths);

        let mut fake = FakeProbe::new();
        for (path, node) in found.iter().zip(&live_nodes) {
            let path = path.to_str().expect("fixture paths are UTF-8");
            fake = match node.kind {
                dustpan_core::identity::NodeKind::Dir => fake.dir(path),
                _ => fake.file(path),
            };
        }
        let fake_guard = Guard::new(inputs(), &fake).expect("the policy loads on the fake");
        let fake_nodes = nodes(&fake, &paths);

        let rule = Arc::new(Rule {
            id: RuleId("bench.cache".into()),
            group: GroupId("bench".into()),
            owners: Box::new([]),
            sink: Sink::Delete,
            default_selected: true,
        });
        let shape = |name, guard, probe, nodes| Shape {
            name,
            guard,
            probe: Counting {
                inner: probe,
                nodes: AtomicUsize::new(0),
            },
            nodes,
            times: Vec::new(),
            eligible: 0,
        };
        let mut scan = shape("scan", &live_guard, &live, Some(&live_nodes[..]));
        let mut explain = shape("explain", &live_guard, &live, None);
        let mut cpu = shape("cpu", &fake_guard, &fake, Some(&fake_nodes[..]));
        // Interleaved, so cache state and background load reach every shape alike.
        for _ in 0..ROUNDS {
            scan.round(&paths, &rule);
            explain.round(&paths, &rule);
            cpu.round(&paths, &rule);
        }

        println!(
            "guard bench: {PATHS} paths under {home}, fixture {}",
            if cached { "reused" } else { "built now" }
        );
        let gated = scan.report(PATHS);
        explain.report(PATHS);
        cpu.report(PATHS);
        if gated < BUDGET_PER_SECOND {
            println!("FAIL: scan is below the budget of {BUDGET_PER_SECOND:.0} paths/s");
            ExitCode::FAILURE
        } else {
            println!("ok: scan meets the budget of {BUDGET_PER_SECOND:.0} paths/s");
            ExitCode::SUCCESS
        }
    }
}
