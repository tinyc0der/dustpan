//! Runs the sink-time recheck, survey included, over every entry of a folder in preview mode,
//! and counts what keeps each one. Read-only: a preview holds no disk.
//!
//! `cargo run --release -p dustpan-sys --example survey_caches -- DIR`
//!
//! Each entry of DIR is a candidate of a Delete rule owned by its own name, as M3's cache rows
//! are. The summary separates candidates the discovery verdict already kept from those only the
//! survey kept, and among those, the ones a protection or whitelist row on a name inside kept.
//! That last count decides whether `Pass::descendant` should skip name rows.
use dustpan_core::candidate::{GroupId, Rule, RuleId, Sink};
use dustpan_core::guard::{ContentsReason, Guard, GuardInputs, Homes, KeptReason, Layer, Lease};
use dustpan_core::oplog::{Command, Journal, Line, RunId};
use dustpan_core::path::{AbsPath, Name};
use dustpan_core::paths::{self, Os};
use dustpan_core::probe::{Owner, Probe, Seen};
use dustpan_core::sink::{Cancel, Executor, Mode, Outcome, Services};
use dustpan_core::time::Clock;
use dustpan_sys::{LiveDisk, LiveProbe, SystemClock};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

struct Discard;

impl Journal for Discard {
    fn write(&mut self, _: Line<'_>) {}
}

fn main() {
    let dir = std::env::args_os()
        .nth(1)
        .expect("usage: survey_caches DIR");
    let dir = AbsPath::parse(&dir).expect("DIR must be absolute");
    let env = |key: &str| std::env::var(key).ok();
    let os = Os::current();
    let probe = LiveProbe::new();
    let home = paths::home_dir(os, &env)
        .and_then(|h| AbsPath::parse(h.as_os_str()).ok())
        .expect("HOME");
    let account = dustpan_sys::account_home()
        .ok()
        .flatten()
        .and_then(|h| AbsPath::parse(h.as_os_str()).ok());
    let homes = Homes::new(home, account);
    let whitelist = dustpan_core::whitelist::Whitelist::defaults(os, &homes);
    let inputs = GuardInputs {
        os,
        homes,
        env: &env,
        whitelist,
    };
    let guard = Guard::new(inputs, &probe).expect("guard");

    let mut names: Vec<_> = std::fs::read_dir(dir.as_path())
        .expect("list DIR")
        .flatten()
        .map(|e| e.file_name())
        .collect();
    names.sort();
    let mut pass = guard.pass(&probe);
    let mut plan = Vec::new();
    let mut at_discovery: BTreeMap<String, usize> = BTreeMap::new();
    for raw in &names {
        let Ok(name) = Name::parse(raw) else { continue };
        let path = dir.join(&name);
        let Seen::Found(node) = probe.node(&path) else {
            continue;
        };
        let owner = name.as_os_str().to_string_lossy().into_owned();
        let rule = Arc::new(Rule {
            id: RuleId("survey.cache".into()),
            group: GroupId("survey".into()),
            owners: Box::new([Owner::BundleId(owner.into())]),
            sink: Sink::Delete,
            default_selected: true,
        });
        match pass.candidate(path, node, &rule) {
            Ok(candidate) => match candidate.verdict().kept() {
                Some(reason) => *at_discovery.entry(reason.code().to_owned()).or_default() += 1,
                None => plan.push(candidate),
            },
            Err(judgment) => {
                let code = judgment
                    .verdict()
                    .kept()
                    .map_or("unjudged", KeptReason::code);
                *at_discovery.entry(code.to_owned()).or_default() += 1;
            }
        }
    }
    drop(pass);

    let clock = SystemClock::new();
    let cancel = Cancel::default();
    let mut journal = Discard;
    let services = Services {
        guard: &guard,
        probe: &probe,
        clock: &clock,
        lease: Lease::BATCH,
        journal: &mut journal,
        cancel: &cancel,
    };
    let id = RunId::new(clock.wall(), std::process::id(), 0);
    let mut executor =
        Executor::<LiveDisk>::start(Mode::Preview, Command::named("survey"), id, services);
    let eligible = plan.len();
    let start = Instant::now();
    let mut would = 0;
    let mut at_recheck: BTreeMap<String, usize> = BTreeMap::new();
    let mut by_row = Vec::new();
    for candidate in plan {
        let (candidate, outcome) = executor.run(candidate);
        match outcome {
            Outcome::WouldRemove(_) => would += 1,
            Outcome::Kept(KeptReason::Contents(ContentsReason::Holds { at, reason })) => {
                let key = format!("holds-kept/{}/{}", reason.layer().code(), reason.code());
                *at_recheck.entry(key).or_default() += 1;
                let row = matches!(
                    reason.layer(),
                    Layer::Critical | Layer::Protection | Layer::Whitelist
                );
                if row {
                    by_row.push(format!("{}  ->  {at}: {reason}", candidate.path()));
                }
            }
            Outcome::Kept(reason) => {
                *at_recheck.entry(reason.code().to_owned()).or_default() += 1;
            }
            other => *at_recheck.entry(format!("{other:?}")).or_default() += 1,
        }
    }
    executor.finish();
    println!("entries: {}", names.len());
    println!("kept at discovery: {at_discovery:?}");
    println!(
        "eligible at discovery: {eligible}; rechecked and surveyed in {:.2} s",
        start.elapsed().as_secs_f64()
    );
    println!("would be removed: {would}");
    println!("kept at recheck: {at_recheck:?}");
    println!(
        "kept only by a protection or whitelist row on a name inside: {}",
        by_row.len()
    );
    for line in by_row {
        println!("  {line}");
    }
}
