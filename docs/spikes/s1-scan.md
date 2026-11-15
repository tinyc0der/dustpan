# S1: scan engine

Spike S1 asks whether a native parallel walk (`jwalk`, plus `getattrlistbulk`) is faster than Mole's `du -skPx` workers on a real home folder, with the same byte totals. The answer decides whether the M1 scanner shells out to `du` or walks natively.

- Branch: `spike/s1-scan` at `6ed92cc8f6e7e7061297a8d6d4d44f380e5c3d22`.
- Program: `spikes/s1/src/main.rs` (`s1walk`, jwalk 0.8, rayon 1, libc).
- Scripts: `spikes/s1/run.sh` (the benchmark), `summarize.py`, `library-gap.sh`, `semantics.sh`.
- Outputs: `spikes/s1/results/run1/`, `run2/`, `library-gap/` and `semantics/`.
- Machine: Apple M1 Pro, 10 cores, macOS 26.6.2 (25G83), Mole 1.63.0 (`run1/machine.tsv`).

## What the benchmark does

`run.sh` scans all of `~` four ways, five runs each, and rotates which method goes first in each run.

| Method | What it runs |
|---|---|
| `mole` | `mo analyze --json ~`, after deleting `~/.cache/mole/analyzer` so no cached size is reused. The script moves the original cache aside first and puts it back at the end. |
| `du` | `/usr/bin/du -skPx <entry>` for each top-level folder of `~`, at most 4 at once, which is Mole's `duSem`. Top-level files are sized from `lstat`. |
| `jwalk` | A `jwalk` walk on 10 threads with one `lstat` per entry. |
| `bulk` | A `rayon` walk on 10 threads that lists each folder with `getattrlistbulk` and reads `ATTR_FILE_ALLOCSIZE`. |

The three native methods count allocated bytes (`st_blocks * 512` or `ATTR_FILE_ALLOCSIZE`). They count a file with more than one hard link once per `(dev, inode)`, and they do not enter a folder on another device. A single scan of `~` took under 3 minutes for every method, so the spike scanned all of `~` and not a subtree. The slowest scan was a Mole run at 130.0 s (`run1/times.tsv`). The home folder holds about 6.4 million files and 745 GB (`run1/totals.tsv`).

`summarize.py` replaces each top-level entry name, except common macOS and toolchain folders, with a keyed label, because the repository is public.

## Results

| Method | Median wall time, run 1 (range) | Median wall time, run 2 (range) | Time against Mole | Total against Mole |
|---|---|---|---|---|
| `mole` | 107.5 s (100.8 to 130.0) | 103.6 s (100.2 to 117.7) | 1.00 | 0 |
| `du` workers | 59.3 s (57.7 to 64.5) | 58.7 s (57.0 to 62.3) | 0.55, 0.57 | +1.08 %, +1.09 % |
| `jwalk` | 49.1 s (47.6 to 58.6) | 47.9 s (47.4 to 52.6) | 0.46, 0.46 | +0.98 %, +0.98 % |
| `bulk` | 30.9 s (30.0 to 36.5) | 31.0 s (30.2 to 34.6) | 0.29, 0.30 | +0.98 %, +0.98 % |

Every number comes from `run1/summary.tsv` and `run2/summary.tsv`. The second run reproduces the first within 4 percent for every median: Mole moved 3.6 percent, `du` 1.0 percent, `jwalk` 2.4 percent and `bulk` 0.3 percent. The S2 and S3 spikes ran on the same machine during part of the runs, and the rotation spreads that noise over all four methods.

Median totals are 737.56 GB for Mole and 744.79 GB for `bulk` in run 1, and 737.27 GB and 744.48 GB in run 2. `jwalk` and `bulk` agree to within 0.001 percent. Both are 0.10 percent below `du` workers, which count a hard link once per `du` process, so a file linked from two top-level folders counts twice there.

`bulk` reports 153 to 156 errors per run (`run1/totals.tsv`, `run2/totals.tsv`). Those are folders that `open` refused, most likely privacy-protected folders under `~/Library`. That is an inference; the spike did not log their names. `jwalk` reports 2, because it reports a folder it cannot read on the parent entry, and `du` exits non-zero for 3 top-level folders. Mole marks its result `partial`. The totals agree, so all methods read the same files.

Against the S1 rule, `bulk` passes. It takes 0.29 to 0.30 times Mole's time, inside the 1.2 limit. Its total is 0.98 percent above Mole's in both runs, inside the 1 percent limit with little room.

## Why the totals differ from Mole

The gap is concentrated in two rows (`run1/entries.tsv`). For `Library`, Mole reports 264.17 GB and `bulk` 270.58 GB. For `Projects`, Mole reports 118.61 GB and `bulk` 119.70 GB.

`mo analyze --json` does not size a folder with one `du`. It walks each subfolder in Go and counts `min(st_blocks * 512, st_size)` per file. It counts an APFS clone that shares every block with another file only once. It sizes about 150 "folded" folder names, such as `node_modules`, `.git` and `target`, with `du`, which does not dedupe clones (`cmd/analyze/scanner.go`, `countableFileSize` and `shouldFoldDirWithPath`).

Two checks narrow the cause:

- Mole's overview skips `Mobile Documents` and `CloudStorage` under `~/Library`. On this machine those hold 4 MB and 0 bytes (`library-gap/run1.tsv`, `run2.tsv`), so they do not explain 6.4 GB.
- `semantics.sh` runs the same walk with Mole's per-file rule and clone dedupe in every folder (`bulk-mole`). The total drops to 675.70 GB, 8.4 percent below Mole, and `Projects` drops to 82.88 GB (`semantics/run1.tsv`, `run2.tsv`). Clone dedupe in every folder removes about 69 GB. Mole removes much less, because its folded folders are sized by `du` without dedupe. The `bulk-mole` walk took 31.6 s against 30.6 s for `bulk`, so the extra clone attributes cost about 3 percent.

The inference is that the 1 percent gap is the net of Mole's mixed rules, not a counting error in either walker. A native walker can reproduce `du`, Mole, or full clone dedupe at the same speed. M1 has to choose one rule on purpose.

## What was not measured

The limiter of each method was not profiled. The likely limiter is metadata reads in the kernel, because `bulk` replaces one `lstat` per file with one `getattrlistbulk` call per batch and runs about 1.6 times faster than `jwalk`. That is an inference. Mole's time includes a Spotlight query for large files, which the native walks do not run.

Verdict. Walk natively with `getattrlistbulk` on a bounded `rayon` pool, not with `du` workers, because it scanned `~` in 0.29 to 0.30 times Mole's time and came within 0.98 percent of Mole's total; M1 must choose its APFS clone rule on purpose, since counting fully shared clones once moves this home folder's total by about 69 GB (9 percent).
