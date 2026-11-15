# Performance baselines

Each section records one budget from `docs/M0-checklist.md`: the probe, the machine, the runs and what limits the number.

## Guard evaluations per second (M0-3)

**Budget.** At least 200,000 evaluations per second over 100,000 fixture paths. A home folder scan can produce millions of candidates. The bench exits 1 below the budget.

**Probe.** `cargo bench -p dustpan-core --bench guard`. It builds the fixture home with `tests/fixtures/mkhome.sh`, adds 100 cache folders named by bundle id with 1,000 files each, and keeps the tree under `target/tmp`. It walks the tree once outside timing. Then it times three shapes, five rounds each, interleaved, and prints the median of the five.

| Shape | What it times | Gated |
|---|---|---|
| `scan` | `Pass::candidate` with the real probe. The walker supplies each entry's node, so the guard calls `lstat` for directories only. | Yes |
| `explain` | `Pass::evaluate` with the real probe: one more `lstat` per path. | No |
| `cpu` | `Pass::candidate` with the in-memory probe: policy matching alone. | No |

**Baseline.** 1,037,956 evaluations per second for `scan`, the first run's median. That is 5.2 times the budget.

**Runs.** Three runs at code commit `c603478`, 2026-11-15. Each cell is the median of five rounds in paths per second.

| Run | Load average (1 min) | `scan` | `explain` | `cpu` |
|---|---|---|---|---|
| 1 | 2.89 | 1,037,956 | 360,280 | 1,196,402 |
| 2 | 2.75 | 1,019,912 | 348,174 | 1,168,541 |
| 3 | 2.61 | 1,024,511 | 357,683 | 1,191,572 |

Within a run the five `scan` rounds spanned 95.5 ms to 100.4 ms for 100,000 paths. `scan` and `cpu` called `lstat` 0.022 times per path, and `explain` 1.022 times.

**Machine.** Apple M1 Pro, 10 cores, 32 GB, macOS 26.6.2, APFS on the internal SSD. Other agents were building on the machine: the 5-minute load average was about 6. The fixture was reused from an earlier run, so its 102,000 entries were in the kernel's vnode cache.

**The work happened.** Every line prints how many paths came back eligible. `scan` and `explain` report 99,998 of 100,000, and `cpu` 99,999. The two kept paths are the fixture's `com.apple.Safari` cache folder, kept by name, and the file in it, kept because Safari was running on the machine. The in-memory probe has no processes, so it keeps only the folder.

**What limits each number.**

- `scan` is bound by policy CPU on one core, not by the file system. `cpu` runs the same matching with no syscall and is about 15% faster, so the 2,200 directory `lstat` calls and the one process-table snapshot account for the rest. A 2-second sample of the timed region (`sample`, not a reported run) shows the time spread over `Policy::rule` (the trie walk and the name patterns), `memcmp`, owner matching against the process table (once per cache folder), `std::path` component parsing, the allocator and SipHash. No one item dominates.
- `explain` is bound by one `lstat` per path. The sample's top frame is `lstat`. This number depends on the vnode cache. The design measured a leaf `lstat` at 5.5 to 67 microseconds on 100,000 cold paths under `~/Library/Caches` with the cache full, which is 15,000 to 181,000 paths per second. That is why the scan path takes the walker's node and the gate is on `scan`.

**History.** The first working build measured 415,627 for `scan` at a load average near 7. Two changes followed: `*x*` bundle patterns are grouped by first byte, and a cache folder's owner is looked up without building an owner value per file. The same fixture then measured 874,594 at the same load.

**Not measured.** Linux and Windows. A cold fixture. More than one thread: a scan uses one `Pass` per worker, and the passes share nothing but the read-only policy.
