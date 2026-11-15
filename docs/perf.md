# Performance baselines

Each section records one budget from `docs/M0-checklist.md`: the probe, the machine, the runs and what limits the number.

## Guard evaluations per second (M0-3)

**Budget.** At least 200,000 evaluations per second over 100,000 fixture paths. A home folder scan can produce millions of candidates. The bench exits 1 below the budget.

**Probe.** `cargo bench -p dustpan-core --bench guard`. It builds the fixture home with `tests/fixtures/mkhome.sh`, adds 100 cache folders named by bundle id with 1,000 files each, and keeps the tree under `target/tmp`. It walks the tree once outside timing. Then it times three shapes, five rounds each, interleaved, and prints the median of the five.

| Shape | What it times | Gated |
|---|---|---|
| `scan` | `Pass::candidate` with the real probe. The walker supplies each entry's node, so the guard calls `lstat` for directories only. | Yes |
| `explain` | `Pass::evaluate` with the real probe: one more `lstat` per path, and one stored-spelling question (`getattrlist`) per path since fix round 1. | No |
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

**After fix round 1 (aliases, volfs, running owners).** `scan` still passes at 4.2 times the budget: 845,779 evaluations per second, the median of three runs. It is 9% slower than `c964f04` measured side by side.

Three runs of each side, alternated (before, after, before, after, before, after), 2026-11-15, same machine. "Before" is an export of `c964f04`; "after" is the fix round 1 tree. Each cell is the median of five rounds in paths per second. The load is the 1-minute average when the run started; another agent's build raised it from run 2.

| Run | Side | Load | `scan` | `explain` | `cpu` |
|---|---|---|---|---|---|
| 1 | before | 1.86 | 929,364 | 360,110 | 1,006,093 |
| 1 | after | 3.39 | 820,645 | 196,148 | 1,092,695 |
| 2 | before | 8.22 | 918,016 | 352,760 | 986,886 |
| 2 | after | 7.74 | 845,779 | 194,753 | 1,089,158 |
| 3 | before | 7.52 | 932,090 | 359,575 | 1,000,448 |
| 3 | after | 6.67 | 896,338 | 216,647 | 1,129,851 |

- `scan` asked for a stored spelling 0.000 times per path and called `lstat` 0.022 times, as before: every fixture name is ASCII, so the scan shape pays nothing per path for the alias check. Its loss is one fixed cost per pass. The process snapshot now reads each running bundle's `Info.plist`. Seven snapshots in one process, 764 processes, two runs per side, alternated: median 12.4 and 12.8 ms after against 3.3 and 3.1 ms before. The first snapshot in a process took 34 to 37 ms after and 6.5 ms before. The median `scan` round grew by 10 to 14 ms in runs 1 and 2, which matches. On a longer scan the cost shrinks toward nothing; at the sink it is paid per owned candidate (see the design's open questions).
- `cpu` is 9% to 13% faster after. It has no processes and no syscalls, so this is policy CPU alone: bundle patterns now match the folded name the pass already holds. The gap is larger than the spread between runs of one side (2%), but no profile was taken to confirm the cause, so treat the cause as inferred.
- `explain` fell from about 360,000 to about 196,000 per second: it now makes two syscalls per path, `lstat` and `getattrlist`, and both asked 1.000 times per path. It is not the gated shape. `explain` and `recheck` always ask, because they are the two places where a spelling arrives from outside a directory read.
- The work happened: every side reports 99,998 of 100,000 eligible for `scan` and `explain`, and 99,999 for `cpu`, the same two kept paths as the baseline.

**History.** The first working build measured 415,627 for `scan` at a load average near 7. Two changes followed: `*x*` bundle patterns are grouped by first byte, and a cache folder's owner is looked up without building an owner value per file. The same fixture then measured 874,594 at the same load.

**Not measured.** Linux and Windows. A cold fixture. More than one thread: a scan uses one `Pass` per worker, and the passes share nothing but the read-only policy.
