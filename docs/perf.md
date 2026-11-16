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

**After fix round 3 (open handles, capped bundle reads).** `scan` is 12% slower and passes at 3.4 times the budget: 772,544 before and 679,373 after, medians of five runs each. The ranges do not overlap (729,563 to 789,328 before, 640,718 to 699,821 after).

Five runs of each side, alternated, 2026-11-16, same machine, load average 4.5 to 7.5 from other agents' builds. "Before" is an export of `a2b9f07` and "after" an export of the fix round 3 tree, in two sibling folders whose paths have the same length. Each cell is the median of five rounds in paths per second. The last column is the number of paths in a snapshot of open files taken right before the run.

| Run | Side | Load | `scan` | `explain` | `cpu` | Open paths |
|---|---|---|---|---|---|---|
| 1 | before | 7.49 | 771,285 | 181,135 | 936,796 | 6,024 |
| 1 | after | 6.96 | 691,965 | 177,291 | 922,530 | 6,024 |
| 2 | before | 6.11 | 789,328 | 183,176 | 941,005 | 6,036 |
| 2 | after | 5.86 | 679,271 | 180,994 | 911,709 | 6,020 |
| 3 | before | 5.49 | 780,234 | 183,577 | 949,211 | 6,034 |
| 3 | after | 4.80 | 679,373 | 174,539 | 930,304 | 6,013 |
| 4 | before | 4.53 | 729,563 | 174,905 | 933,438 | 6,038 |
| 4 | after | 4.87 | 640,718 | 170,328 | 900,138 | 6,223 |
| 5 | before | 5.28 | 772,544 | 180,291 | 940,906 | 6,121 |
| 5 | after | 5.24 | 699,821 | 178,180 | 925,937 | 6,122 |

- The loss is a fixed cost per pass. The median `scan` round grew from 129.4 ms to 147.2 ms, 18 ms for 100,000 paths. One snapshot of open files costs 11 to 13 ms on this host (below), and sorting its 6,000 paths and asking it once per folder is the rest. On a longer scan the cost per path shrinks toward nothing.
- The snapshot itself: 11.0, 12.7 and 13.1 ms, each the median of seven snapshots in one process (`cargo run --release -p dustpan-sys --example process_table`), with 836 processes, 6,206 paths and 541 processes of this user listed. A separate count on the same host found 7,021 descriptors in 509 listed processes: 5,493 files and folders and 552 sockets, of which 110 are Unix sockets bound to a path. The 295 processes of other users refuse the list at once and cost one call each.
- What limits the snapshot is one kernel call per descriptor, about 1.6 µs each. While a Docker VM held 102,000 files of a shared folder open, the snapshot took 181 ms for 114,977 paths, and with one more process that held 50,000 descriptors it took 225 ms for 157,268. No kernel limit holds the number of descriptors down: `kern.maxfiles` counts open files, and `dup` and `fork` add descriptors that share one. So a snapshot stops at 262,144 descriptors and answers "timed out", which keeps every path no rule protects. At 1.6 µs each that is about 0.4 s, which is arithmetic, not a measurement.
- The pass asks the snapshot once per folder, not once per path: a folder with nothing open in it answers for its files. Without that, five alternated runs measured 776,413 before and 564,364 after, 27% slower (`bench-ab-lookup-per-path.txt`). A temporary switch in the bench split that 48 ms per round into the snapshot (10 to 17 ms) and the sort and lookups (about 27 ms, 270 ns per path) by feeding the pass a cached snapshot and then an empty one (`bench-explain.txt`).
- A folder that is held costs more. With the Docker VM holding every file of the fixture, `scan` measured 275,207 per second in one run, with 0 of 100,000 paths eligible: the snapshot was 181 ms and every path was looked up and found. It still met the budget. One run, not a comparison.
- `explain` shows no measurable difference: 181,135 and 177,291, and the ranges overlap. It takes the same one snapshot per pass.
- `cpu` is 2% slower (940,906 to 922,530, ranges 933,438 to 949,211 and 900,138 to 930,304). The in-memory probe has no open files, so this is the one lookup per folder against an empty index and the larger frame. No profile was taken, so the cause is inferred.
- The process snapshot did not change on this host: 46 ms before and 45 ms after for 844 processes (the first snapshot in a process, one run of seven processes before and one after). The caps on `Info.plist` reads are far above what its processes need (242 files, 0.77 MB against 8 MiB). With the round 3 fixture running, `explain` took 6.32 s before and 0.05 s after with one process, and 31.46 s and 0.05 s with five.
- The work happened: every run of both sides reports 99,998 of 100,000 eligible for `scan` and `explain` and 99,999 for `cpu`. No run failed the budget.
- At the sink the snapshot is paid per candidate, on top of the process table (see the design's open questions).
- The evidence files are in `/tmp/swarm-m0-3/fix3/`.

**History.** The first working build measured 415,627 for `scan` at a load average near 7. Two changes followed: `*x*` bundle patterns are grouped by first byte, and a cache folder's owner is looked up without building an owner value per file. The same fixture then measured 874,594 at the same load.

**After fix round 4 (a held file the snapshot cannot place).** No measurable difference for `scan`: 660,862 before and 664,166 after, medians of five runs each, and the ranges overlap (451,097 to 697,492 before, 608,524 to 700,379 after). It passes at 3.3 times the budget.

Five runs of each side, alternated, 2026-11-16, same machine, load average 2.9 to 8.5 from other agents' builds. "Before" is an export of `f3c8ce7` and "after" an export of the fix round 4 tree, in two sibling folders whose paths have the same length. Each cell is the median of five rounds in paths per second. The last column is the number of paths in a snapshot of open files taken right after the run.

| Run | Side | Load | `scan` | `explain` | `cpu` | Open paths |
|---|---|---|---|---|---|---|
| 1 | before | 2.92 | 451,097 | 121,975 | 745,911 | 6,373 |
| 1 | after | 5.33 | 656,786 | 167,022 | 931,932 | 6,374 |
| 2 | before | 5.12 | 679,191 | 173,645 | 921,049 | 7,175 |
| 2 | after | 7.46 | 608,524 | 151,623 | 916,098 | 9,025 |
| 3 | before | 8.48 | 620,821 | 168,852 | 901,742 | 9,005 |
| 3 | after | 8.48 | 664,166 | 172,457 | 926,418 | 8,996 |
| 4 | before | 8.02 | 660,862 | 171,380 | 951,413 | 8,996 |
| 4 | after | 7.18 | 700,379 | 179,814 | 956,606 | 8,996 |
| 5 | before | 6.23 | 697,492 | 180,713 | 953,191 | 8,986 |
| 5 | after | 5.57 | 696,864 | 182,627 | 961,059 | 8,987 |

- The change adds one `lstat` for each reported path of 768 bytes or more, and the host has none among 6,267, so a snapshot makes the same calls as before. That is why nothing moved. The check for a socket address is the `realpath` round 3 already made.
- Run 1 "before" is an outlier in all three shapes, `cpu` included, which makes no system call. Its five `scan` rounds spanned 162.7 to 298.2 ms. Another agent's build is the likely cause; it was not traced. Without that run the "before" median is 670,027.
- The snapshot of open files took 11.8 to 22.5 ms, each the median of seven in one process, and followed the number of open paths, which rose from 6,373 to about 9,000 during the runs on both sides.
- The work happened: every run of both sides reports 99,998 of 100,000 eligible for `scan` and `explain` and 99,999 for `cpu`, and no process of this user was unlisted. No run failed the budget.

**Not measured.** Linux and Windows. A cold fixture. More than one thread: a scan uses one `Pass` per worker, and the passes share nothing but the read-only policy.
