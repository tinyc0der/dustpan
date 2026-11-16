//! `cargo run -p dustpan-sys --example process_table`: what one process snapshot and one
//! snapshot of open files hold on this host, and what each costs. An unidentified process of
//! this user keeps every owned cache (SAFE-08), and one that lists no open files keeps every
//! path no rule protects (SAFE-09), so this prints each one. CI runs it on every runner image.
use dustpan_core::probe::{App, Probe, Proc, Seen};
use dustpan_sys::LiveProbe;
use std::collections::HashSet;
use std::time::{Duration, Instant};

const SNAPSHOTS: usize = 7;

/// The first run and the median of `SNAPSHOTS` runs, in milliseconds.
fn timed<T>(mut take: impl FnMut() -> T) -> (T, f64, f64) {
    let mut times: Vec<Duration> = Vec::new();
    let mut last = None;
    for _ in 0..SNAPSHOTS {
        let start = Instant::now();
        last = Some(take());
        times.push(start.elapsed());
    }
    let first = times[0].as_secs_f64() * 1e3;
    times.sort();
    let median = times[SNAPSHOTS / 2].as_secs_f64() * 1e3;
    (last.expect("SNAPSHOTS is not zero"), first, median)
}

fn main() {
    let probe = LiveProbe::new();
    processes(&probe);
    open_files(&probe);
}

fn processes(probe: &LiveProbe) {
    let (table, first, median) = timed(|| probe.processes());
    let table = match table {
        Seen::Found(table) => table,
        other => {
            println!("process table: {other:?}");
            return;
        }
    };
    let in_bundle = |app: &App| matches!(app, App::Bundles(ids) if !ids.is_empty());
    let unknown = |mine: bool| {
        table
            .procs
            .iter()
            .filter(move |p| p.app == App::Unknown { mine })
    };
    let count = |holds: fn(&Proc) -> bool| table.procs.iter().filter(|p| holds(p)).count();
    println!(
        "process table: {} processes, first snapshot {first:.1} ms, median of {SNAPSHOTS} \
         {median:.1} ms, complete: {}, in a bundle: {}, with a signing identifier: {}, \
         entitled to an app group: {}, unidentified: {} of this user, {} of other users",
        table.procs.len(),
        table.complete,
        table.procs.iter().filter(|p| in_bundle(&p.app)).count(),
        count(|p| p.signing_id.is_some()),
        count(|p| !p.app_groups.is_empty()),
        unknown(true).count(),
        unknown(false).count(),
    );
    for (mine, whose) in [(true, "this user"), (false, "another user")] {
        for p in unknown(mine) {
            println!("  unidentified ({whose}): pid {} {:?}", p.pid.0, p.exe);
        }
    }
}

fn open_files(probe: &LiveProbe) {
    let (open, first, median) = timed(|| probe.open_files());
    let open = match open {
        Seen::Found(open) => open,
        other => {
            println!("open files: {other:?}");
            return;
        }
    };
    let holders: HashSet<_> = open.files.iter().map(|file| file.pid).collect();
    let unlisted = |mine: bool| open.unlisted.iter().filter(move |p| p.mine == mine);
    println!(
        "open files: {} paths held by {} processes, first snapshot {first:.1} ms, median of \
         {SNAPSHOTS} {median:.1} ms, unlisted: {} of this user, {} of other users",
        open.files.len(),
        holders.len(),
        unlisted(true).count(),
        unlisted(false).count(),
    );
    for process in unlisted(true) {
        println!("  lists no open files (this user): pid {}", process.pid.0);
    }
}
