//! `cargo run -p dustpan-sys --example process_table`: what one process snapshot holds on
//! this host. An unidentified process of this user keeps every owned cache (SAFE-08), so this
//! prints each one. CI runs it on every runner image.
use dustpan_core::probe::{App, Probe, Seen};
use dustpan_sys::LiveProbe;
use std::time::Instant;

fn main() {
    let start = Instant::now();
    let table = LiveProbe::new().processes();
    let took = start.elapsed();
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
    println!(
        "process table: {} processes in {:.1} ms, complete: {}, in a bundle: {}, \
         unidentified: {} of this user, {} of other users",
        table.procs.len(),
        took.as_secs_f64() * 1e3,
        table.complete,
        table.procs.iter().filter(|p| in_bundle(&p.app)).count(),
        unknown(true).count(),
        unknown(false).count(),
    );
    for (mine, whose) in [(true, "this user"), (false, "another user")] {
        for p in unknown(mine) {
            println!("  unidentified ({whose}): pid {} {:?}", p.pid.0, p.exe);
        }
    }
}
