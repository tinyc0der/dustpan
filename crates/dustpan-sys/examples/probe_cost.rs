//! Measures what the guard's layer 1 costs in syscalls on a real tree, read-only.
//!
//! `cargo run --release -p dustpan-sys --example probe_cost -- DIR [LIMIT]`
//!
//! Collects up to LIMIT paths under DIR in depth-first order, then times `LiveProbe::node` three
//! ways, interleaved over five rounds:
//! - `walker`: lstats each directory once and no leaf, because the walker already read the leaf's
//!   type and id (the `Pass::candidate` path during a scan)
//! - `shared`: lstats each directory once and each leaf once (`Pass::evaluate`, as `explain` does)
//! - `unshared`: lstats every ancestor of every path (no sharing at all)
use dustpan_core::path::AbsPath;
use dustpan_core::probe::Probe;
use dustpan_sys::LiveProbe;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn main() {
    let mut args = std::env::args_os().skip(1);
    let root = PathBuf::from(args.next().expect("usage: probe_cost DIR [LIMIT]"));
    let limit: usize = args
        .next()
        .map_or(100_000, |s| s.to_str().unwrap().parse().unwrap());
    let mut paths = Vec::new();
    walk(&root, &mut paths, limit);
    let paths: Vec<AbsPath> = paths
        .iter()
        .map(|p| AbsPath::parse(p.as_os_str()).unwrap())
        .collect();
    let probe = LiveProbe::new();

    let (mut walker, mut shared, mut unshared) = (Vec::new(), Vec::new(), Vec::new());
    let (mut walker_calls, mut shared_calls, mut unshared_calls) = (0, 0, 0);
    for _ in 0..5 {
        let (t, n) = run_shared(&probe, &paths, false);
        walker.push(t);
        walker_calls = n;
        let (t, n) = run_shared(&probe, &paths, true);
        shared.push(t);
        shared_calls = n;
        let (t, n) = run_unshared(&probe, &paths);
        unshared.push(t);
        unshared_calls = n;
    }
    report("walker", &mut walker, walker_calls, paths.len());
    report("shared", &mut shared, shared_calls, paths.len());
    report("unshared", &mut unshared, unshared_calls, paths.len());
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>, limit: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        if out.len() >= limit {
            return;
        }
        let path = entry.path();
        let is_dir = entry.file_type().is_ok_and(|t| t.is_dir());
        out.push(path.clone());
        if is_dir {
            walk(&path, out, limit);
        }
    }
}

fn run_shared(probe: &LiveProbe, paths: &[AbsPath], leaf: bool) -> (Duration, usize) {
    let start = Instant::now();
    let mut stack: Vec<AbsPath> = Vec::new();
    let mut calls = 0;
    for path in paths {
        let parent = path.parent().unwrap();
        while stack.last().is_some_and(|top| !parent.starts_with(top)) {
            stack.pop();
        }
        let mut missing: Vec<AbsPath> = Vec::new();
        let mut cur = Some(parent);
        while let Some(dir) = cur {
            if stack.last().is_some_and(|top| top == &dir) {
                break;
            }
            cur = dir.parent();
            missing.push(dir);
        }
        for dir in missing.into_iter().rev() {
            std::hint::black_box(probe.node(&dir));
            calls += 1;
            stack.push(dir);
        }
        if leaf {
            std::hint::black_box(probe.node(path));
            calls += 1;
        }
    }
    (start.elapsed(), calls)
}

fn run_unshared(probe: &LiveProbe, paths: &[AbsPath]) -> (Duration, usize) {
    let start = Instant::now();
    let mut calls = 0;
    for path in paths {
        let mut cur = Some(path.clone());
        while let Some(p) = cur {
            std::hint::black_box(probe.node(&p));
            calls += 1;
            cur = p.parent();
        }
    }
    (start.elapsed(), calls)
}

fn report(name: &str, times: &mut [Duration], calls: usize, paths: usize) {
    times.sort();
    let median = times[times.len() / 2];
    let per_path = median.as_nanos() as f64 / paths as f64;
    println!(
        "{name:>8}: {paths} paths, {calls} lstat calls ({:.2} per path), median {:.1} ms, \
         {per_path:.0} ns per path, {:.0} paths/s (min {:.1} ms, max {:.1} ms)",
        calls as f64 / paths as f64,
        median.as_secs_f64() * 1e3,
        1e9 / per_path,
        times[0].as_secs_f64() * 1e3,
        times[times.len() - 1].as_secs_f64() * 1e3,
    );
}
