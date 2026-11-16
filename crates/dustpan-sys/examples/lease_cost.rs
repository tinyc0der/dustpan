//! Measures what the evidence lease saves at the sink, read-only.
//!
//! `cargo run --release -p dustpan-sys --example lease_cost`
//!
//! Builds 200 small cache folders in a temporary folder, then runs one preview-mode
//! executor over all of them with `Lease::NONE` (a process table and a snapshot of open files
//! per candidate) and with `Lease::BATCH` (one of each per second), interleaved over five rounds.
//! A preview holds no disk, so nothing is removed. Also times the two snapshots alone.
#[path = "support/fixture_dir.rs"]
mod fixture_dir;

use dustpan_core::candidate::{Candidate, GroupId, Rule, RuleId, Sink};
use dustpan_core::guard::{Guard, GuardInputs, Homes, Lease};
use dustpan_core::oplog::{Command, Journal, Line, RunId};
use dustpan_core::path::AbsPath;
use dustpan_core::paths::Os;
use dustpan_core::probe::{Owner, Probe, Seen};
use dustpan_core::sink::{Cancel, Executor, Mode, Outcome, Services};
use dustpan_core::time::Clock;
use dustpan_core::whitelist::Whitelist;
use dustpan_sys::{LiveDisk, LiveProbe, SystemClock};
use fixture_dir::FixtureDir;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

const CANDIDATES: usize = 200;
const ROUNDS: usize = 5;

struct Discard;

impl Journal for Discard {
    fn write(&mut self, _: Line<'_>) {}
}

fn plan(guard: &Guard, probe: &LiveProbe, home: &Path) -> Vec<Candidate> {
    // An owner per rule, as M3's catalog rows have, so every recheck asks the process table.
    let rule = Arc::new(Rule {
        id: RuleId("lease.cache".into()),
        group: GroupId("lease".into()),
        owners: Box::new([Owner::BundleId("com.example.lease".into())]),
        sink: Sink::Delete,
        default_selected: true,
    });
    let mut pass = guard.pass(probe);
    (0..CANDIDATES)
        .map(|i| {
            let path =
                AbsPath::parse(home.join(format!("Library/Caches/c{i:03}")).as_os_str()).unwrap();
            let Seen::Found(node) = probe.node(&path) else {
                panic!("no {path}")
            };
            pass.candidate(path, node, &rule).unwrap()
        })
        .collect()
}

fn preview(
    guard: &Guard,
    probe: &LiveProbe,
    plan: Vec<Candidate>,
    lease: Lease,
) -> (Duration, usize) {
    let clock = SystemClock::new();
    let cancel = Cancel::default();
    let mut journal = Discard;
    let services = Services {
        guard,
        probe,
        clock: &clock,
        lease,
        journal: &mut journal,
        cancel: &cancel,
    };
    let start = Instant::now();
    let id = RunId::new(clock.wall(), std::process::id(), 0);
    let mut executor =
        Executor::<LiveDisk>::start(Mode::Preview, Command::named("lease"), id, services);
    let would = plan
        .into_iter()
        .map(|candidate| executor.run(candidate).1)
        .filter(|outcome| matches!(outcome, Outcome::WouldRemove(_)))
        .count();
    executor.finish();
    (start.elapsed(), would)
}

fn median(mut runs: Vec<Duration>) -> Duration {
    runs.sort();
    runs[runs.len() / 2]
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1_000.0
}

fn main() {
    let fixture = FixtureDir::new("dustpan-lease-cost-");
    let home = fixture.path();
    for i in 0..CANDIDATES {
        let dir = home.join(format!("Library/Caches/c{i:03}"));
        std::fs::create_dir_all(&dir).unwrap();
        for f in 0..5 {
            std::fs::write(dir.join(format!("f{f}")), b"x").unwrap();
        }
    }
    let probe = LiveProbe::new();
    let env = |_: &str| None;
    let abs_home = AbsPath::parse(home.as_os_str()).unwrap();
    let inputs = GuardInputs {
        os: Os::current(),
        homes: Homes::new(abs_home.clone(), None),
        env: &env,
        whitelist: Whitelist::defaults(Os::current(), &abs_home),
    };
    let guard = Guard::new(inputs, &probe).unwrap();

    let (mut none, mut batch, mut snapshots) = (Vec::new(), Vec::new(), Vec::new());
    let mut eligible = (0, 0);
    for _ in 0..ROUNDS {
        let start = Instant::now();
        let _ = probe.processes();
        let _ = probe.open_files();
        snapshots.push(start.elapsed());
        let (t, n) = preview(&guard, &probe, plan(&guard, &probe, home), Lease::NONE);
        none.push(t);
        eligible.0 = n;
        let (t, n) = preview(&guard, &probe, plan(&guard, &probe, home), Lease::BATCH);
        batch.push(t);
        eligible.1 = n;
    }
    let per = |d: Duration| ms(d) / CANDIDATES as f64;
    let (none, batch, snapshot) = (median(none), median(batch), median(snapshots));
    println!(
        "process table + open files, one each: median {:.1} ms over {ROUNDS} rounds",
        ms(snapshot)
    );
    println!(
        "Lease::NONE: {CANDIDATES} rechecks in {:.0} ms ({:.2} ms each), {} would be removed",
        ms(none),
        per(none),
        eligible.0
    );
    println!(
        "Lease::BATCH: {CANDIDATES} rechecks in {:.0} ms ({:.2} ms each), {} would be removed",
        ms(batch),
        per(batch),
        eligible.1
    );
    println!(
        "a 1 s lease spends at most {:.1} percent of each second on snapshots",
        ms(snapshot) / 10.0
    );
}
