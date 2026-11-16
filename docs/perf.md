# Performance baselines

Each section records one budget from `docs/M0-checklist.md`: the probe, the machine, the runs and what limits the number.

## Guard evaluations per second (M0-3)

**Budget.** At least 200,000 evaluations per second over 100,000 fixture paths. A home folder scan can produce millions of candidates. The bench exits 1 below the budget.

**Probe.** `cargo bench -p dustpan-core --bench guard`. It builds the fixture home with `tests/fixtures/mkhome.sh`, adds 100 cache folders named by bundle id with 1,000 files each, and keeps the tree under `target/tmp`. It walks the tree once outside timing. Then it times three shapes, five rounds each, interleaved, and prints the median of the five.

| Shape | What it times | Gated |
|---|---|---|
| `scan` | `Pass::candidate` with the real probe. The walker supplies each entry's node, so the guard calls `lstat` for directories only. One process snapshot per round. | Yes |
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

**After fix round 2 (bounded bundle reads, app groups, layout words).** No measurable difference for `scan`: 795,105 before and 783,809 after, medians of five runs each, and the two ranges overlap (770,352 to 824,369 before, 766,467 to 822,719 after). It passes at 3.9 times the budget.

Five runs of each side, alternated (before, after, and so on), 2026-11-16, same machine, load average 2.5 to 2.9. "Before" is an export of `1f8c9f9` and "after" an export of `23b1c8c`, in two sibling folders whose paths have the same length. Each cell is the median of five rounds in paths per second. The last column is the median of seven process snapshots in one process, taken right after the run.

| Run | Side | Load | `scan` | `explain` | `cpu` | Snapshot (ms) |
|---|---|---|---|---|---|---|
| 1 | before | 2.81 | 796,992 | 198,282 | 925,433 | 15.8 |
| 1 | after | 2.67 | 822,719 | 194,722 | 964,799 | 39.2 |
| 2 | before | 2.81 | 795,105 | 192,090 | 942,708 | 15.6 |
| 2 | after | 2.74 | 766,467 | 184,047 | 933,754 | 35.6 |
| 3 | before | 2.70 | 789,501 | 193,776 | 952,637 | 12.9 |
| 3 | after | 2.90 | 779,808 | 188,056 | 963,113 | 32.9 |
| 4 | before | 2.92 | 770,352 | 192,220 | 949,738 | 11.7 |
| 4 | after | 2.86 | 812,259 | 194,058 | 961,043 | 33.2 |
| 5 | before | 2.65 | 824,369 | 200,883 | 952,594 | 12.0 |
| 5 | after | 2.52 | 783,809 | 187,892 | 940,248 | 32.8 |

- Two fixed costs per pass moved in opposite directions, and both were measured inside the bench with a temporary timer (`/tmp/swarm-m0-3/fix2/bench-explain.txt`). The process snapshot grew from 14 ms to 36 ms: it now asks the kernel for each of about 800 processes' signing identifier and entitlements (two `csops` calls each). Owner matching for the fixture's 100 cache folders fell from 37 ms to 16 ms per round: a process in a bundle is compared by id and no longer has its path lowercased and searched for the id's words. The sum is 51 ms before and 52 ms after, which is why `scan` did not move.
- On a longer scan both costs shrink toward nothing per path. At the sink the snapshot is paid per owned candidate, and it is now 36 ms (see the design's open questions).
- `explain` is 3% slower (193,776 to 188,056, medians), and its ranges touch (192,090 to 200,883 before, 184,047 to 194,722 after), so treat it as at most a small loss. It is not the gated shape.
- `cpu` did not change: 949,738 and 961,043, inside each side's range. It has no processes.
- The work happened: every run of both sides reports 99,998 of 100,000 eligible for `scan` and `explain` and 99,999 for `cpu`, with 0.022 `lstat` calls per path for `scan`. No run failed the budget.
- Where the fixture sits changes every number. A first set of five alternated runs had "before" in a folder with a longer path than "after" (`bench-ab-confounded.txt`): `cpu` read 964,482 against 1,125,649, a 17% gap that the side-by-side runs do not show. The cause is inferred, not profiled: the in-memory probe and the policy both hash and compare whole path strings, and the paths of that folder are 45 bytes longer. Fix round 1's comparison above had the same layout, so its "9% slower" for `scan` and "9% to 13% faster" for `cpu` both carry that bias. Compare two sides only from folders whose paths have the same length.

**History.** The first working build measured 415,627 for `scan` at a load average near 7. Two changes followed: `*x*` bundle patterns are grouped by first byte, and a cache folder's owner is looked up without building an owner value per file. The same fixture then measured 874,594 at the same load.

**Not measured.** Linux and Windows. A cold fixture. More than one thread: a scan uses one `Pass` per worker, and the passes share nothing but the read-only policy.

## Guard with a 500-line whitelist (M0-4)

**Budget.** The head's `scan` median must not be more than 25 percent below the M0-3 head's (`a2b9f07`), with a 500-line whitelist loaded.

**Probe.** `cargo bench -p dustpan-core --bench guard`, from two exports of the repository in sibling folders with paths of the same length (`side-a` holds `a2b9f07`, `side-b` holds `c5dd004`). The M0-3 bench has no whitelist, so the head's bench loads 500 user lines (250 literals under `~/Projects`, 150 literal cache folders, 100 cache globs beside the fixture's) that name none of the fixture paths. Every path is still judged eligible, so the number is what layer 5 costs to say no. The bench checks that a path the whitelist names is kept.

**Baseline.** Trunk median for `scan`, recorded first: 822,299 evaluations per second.

**Runs.** Three runs of each side, alternated (trunk, head, trunk, head, trunk, head), 2026-11-16. Each cell is the median of five rounds in paths per second.

| Run | Side | Load (1 min) | `scan` | `explain` | `cpu` |
|---|---|---|---|---|---|
| 1 | trunk | 5.77 | 814,402 | 210,888 | 1,265,907 |
| 1 | head | 5.63 | 827,668 | 211,821 | 1,289,325 |
| 2 | trunk | 5.44 | 826,197 | 211,641 | 1,296,806 |
| 2 | head | 5.57 | 828,388 | 210,286 | 1,282,668 |
| 3 | trunk | 5.33 | 822,299 | 199,214 | 1,296,313 |
| 3 | head | 5.22 | 750,732 | 185,167 | 1,233,515 |

Medians of the three runs: `scan` 822,299 on trunk and 827,668 on head (0.7% faster, inside the spread); `explain` 210,888 and 210,286; `cpu` 1,296,313 and 1,282,668. The rule passes by a wide margin. Run 3 of the head was the slowest cell, and its trunk neighbour was the slowest trunk cell for `explain`, so a background process most likely caused both.

**What limits the number.** A `Pass` keeps one position in the whitelist trie per ancestor folder, so a path outside every entry costs one comparison and a path in a folder with entries costs one hash lookup plus a glob match per glob in that folder. A first version walked every component from the top for each path and measured 445,000 for `scan` (48% slower), almost all of it per-component allocation. The 100 cache globs are matched only for the 100 cache folders, not for the 100,000 files below them.

**The work happened.** Every run reports 99,998 of 100,000 eligible for `scan` and `explain` on both sides, the same count as M0-3.

**Machine.** Apple M1 Pro, 10 cores, 32 GB, macOS 26.6.2. Other agents were building on the machine, load average 5.2 to 5.8.

### Fix round 1

The fix round folds non-ASCII names, matches `?` and `[...]`, and resolves every entry through the probe at `Guard::new`. The same bench ran from fresh exports (`side-a` at `a2b9f07`, `side-c` at `5126bf5`) under `/tmp/swarm-m0-4/fix1/bench`, alternated, three runs each, on 2026-11-16.

| Run | Side | Load (1 min) | `scan` | `explain` | `cpu` |
|---|---|---|---|---|---|
| 1 | trunk | 7.15 | 806,668 | 200,358 | 1,231,661 |
| 1 | head | 6.60 | 845,294 | 203,661 | 1,254,655 |
| 2 | trunk | 6.31 | 842,309 | 215,239 | 1,294,971 |
| 2 | head | 7.42 | 840,201 | 212,537 | 1,244,352 |
| 3 | trunk | 6.99 | 899,543 | 204,217 | 1,271,496 |
| 3 | head | 6.07 | 870,925 | 213,700 | 1,252,601 |

Medians: `scan` 842,309 on trunk and 845,294 on head (0.4% faster, inside the spread), `explain` 204,217 and 212,537, `cpu` 1,271,496 and 1,252,601 (1.5% slower). The rule passes. An earlier round of three runs per side, taken while the fixtures were first built, swung from 433,003 to 802,047 for `scan` on both sides. Its `cpu` medians were 1,239,496 on trunk and 1,227,168 on head.

**What limits the number.** The same as above. An all-ASCII name takes the ASCII fold, so the bench's names cost what they did. Resolution runs at `Guard::new`, outside the timed rounds. `explain` as a process, which includes it, took 11.6 ms at this head and 9.1 ms at `13d50e0`, as means of 100 alternated runs with the same 500 lines.

**The work happened.** Every run reports 99,998 of 100,000 eligible for `scan` and `explain`, and the bench still checks that a path the whitelist names is kept.
