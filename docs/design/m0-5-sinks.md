# M0-5 sinks, executor, operations log and `history`

## Problem

M0-5 builds the first code in dustpan that removes anything. The Delete and Trash sinks take an `Approved` from M0-3's guard. An executor runs a reviewed plan in preview or apply mode. Every mutation and every refusal writes one row in Mole's operations log grammar, and `dustpan history` reads that log back with Mole's HIST-02 field names. M3 (clean), M5 (uninstall), M6 (purge, installers) and the M4 privileged helper all call this code.

Five constraints make the shape non-obvious.

1. A sink takes `Approved` by value and nothing else, and preview shares every step but the sink call (SAFE-18). Anything that can refuse must therefore run before the `Approved` exists. That includes the judgment of what a folder candidate holds (SAFE-27, SAFE-38). The guard judged the root path, not the files inside it.
2. `dustpan-core` does no I/O and forbids `unsafe`. `dustpan-sys` holds every OS call. Yet the delete walk's safety lives in the order of calls (`openat` with `O_NOFOLLOW | O_DIRECTORY`, `fstat`, `unlinkat`), and the unit tests in `crates/dustpan-core/tests/sink.rs` need a swapped inode, a parent replaced by a link and a mount inside the target.
3. M0-3's recheck built a fresh pass per candidate, so it took one process table and one open-files snapshot per candidate: about 57 ms each on the bench host, 57 s per thousand candidates.
4. The workspace `clippy.toml` banned `std::fs` removals outside the sink module, but the sink itself calls `libc::unlinkat`, which no ban covered.
5. The M4 helper will run the same sink as root after its own recheck. The sink may depend only on things the helper can build. The helper cannot share the parent's log file, process table or guard.

## Usage (caller's view)

`crates/dustpan-core/tests/usage.rs` type-checks these call sites.

**M3 `clean`, preview and apply.** The only branch on `--dry-run` is which `Mode` is built. A preview holds no disk, so it cannot remove anything.

```rust
let mut log = OpLog::new(FileStore::new(log_dir), |w: &LogWarning| eprintln!("dustpan: {w}"));
let mode = if dry_run { Mode::Preview } else { Mode::Apply(&LiveDisk::new()) };
let run = RunId::new(clock.wall(), std::process::id(), nonce);
let services = Services { guard: &guard, probe: &probe, clock: &clock, lease: Lease::BATCH,
                          journal: &mut log, cancel: &cancel };
let mut executor = Executor::start(mode, Command::CLEAN, run, services);   // start marker
for candidate in plan {
    let (candidate, outcome) = executor.run(candidate);                  // one row, if any
    tui.show(&candidate, &outcome);
}
let tally = executor.finish();                                          // end marker
```

**`dustpan history`, text and JSON.** It reads `operations.log.old`, then `operations.log`, and never opens either for writing.

```text
$ dustpan history --json | jq .
{ "schema_version": 1, "command": "history",
  "logs": {"operations": ".../operations.log", "deletions": ".../operations.log"},
  "limit": 20,
  "sessions": [{"command": "clean", "run_id": "20261116094500-4242-9f3a", "attribution": "run",
                "started_at": "2026-11-16 09:45:00", "ended_at": "2026-11-16 09:45:02",
                "items": 1, "size": "1.5 MB", "operation_count": 2, "failed_tasks": 0,
                "actions": {"removed": 1, "trashed": 0, "skipped": 1, "failed": 0, "rebuilt": 0, "other": 0}}],
  "deletions": [{"timestamp": "2026-11-16T09:45:01+0700", "mode": "trash", "status": "owner-running",
                 "size_kb": null, "path": "/Users/a/Library/Caches/y"},
                {"timestamp": "2026-11-16T09:45:01+0700", "mode": "permanent", "status": "ok",
                 "size_kb": 1465, "path": "/Users/a/Library/Caches/x"}] }
$ dustpan history --limit 0; echo $?    # 2, and so is --limit 201
```

**The live sink test** (`cargo test -p dustpan-sys --test sink_fixture`). There is no CLI path to a sink in M0 (Appendix B, incident I-64). The test builds a fixture home, discovers with `Pass::candidate`, and runs one `Executor` in apply mode with `LiveDisk` and an `OpLog` over `FileStore`, so `dustpan history` afterwards lists one session with its removed and kept rows. `trash_lane` and `mount_lane` are `#[ignore]`d and run by hand.

**The M4 helper as root.** It builds its own `Guard`, `LiveProbe`, `LiveDisk`, `SystemClock` and `Cancel`, and runs the same `Executor` with `Lease::NONE` and a journal that sends `Line::Item` rows down its pipe and drops the start and end markers. The parent owns the log file and the session, so a root process never creates or rotates a file in the user's log folder.

## Shape

### Data structures

| Type | File | What it guarantees |
|---|---|---|
| `Disk` (trait), `Seal`, `DiskError`, `Unlink`, `Trashed` | `dustpan-core/src/disk.rs` | Six handle-relative calls plus `trash`. Every method takes a `&Seal` only core can build, so only core's sinks can call one. A compile-fail doctest pins it. |
| `DirEntry` | `dustpan-core/src/identity.rs` | One directory entry: the raw name, `Node`, allocated bytes, link count, `mount_root`, `dataless`. Shared by the survey and the delete walk. |
| `Probe::list` | `dustpan-core/src/probe.rs` | A sixth read-only question. Lists a folder opened with no link in any component and returns that folder's own `FileId`. |
| `Recheck`, `Lease` | `dustpan-core/src/guard/approved.rs` | One batch of rechecks. Each candidate gets a fresh pass, fresh identity binds and a fresh survey. Only the process table and the open-files index are shared, each while younger than the lease by `Clock::elapsed`. `Lease::BATCH` is 1 s, `Lease::NONE` shares nothing. |
| `Manifest`, `MAX_DEPTH`, `MAX_ENTRIES`, `DEADLINE` (crate-private) | `dustpan-core/src/guard/survey.rs` | The sorted `FileId`s of every folder the survey judged, carried inside `Approved`, and the survey's bounds. The delete walk enters no other folder. |
| `ContentsReason`, `Bound`, `Layer::Contents` | `dustpan-core/src/guard/verdict.rs` | Survey refusals: `holds-kept` (with the inner reason), `holds-dataless`, `changing`, `too-many-entries`, `too-deep`, `survey-timed-out`. Like `Identity`, only `recheck` produces them. |
| `Mode<'a, D>`, `Outcome`, `Failure`, `FailCause`, `Tally`, `Cancel`, `Services`, `Executor` | `dustpan-core/src/sink.rs` | The executor and the one map from an outcome to its row. `Mode::Apply(&D)` is the only place a disk exists. |
| `sink/delete.rs`, `sink/trash.rs` | `dustpan-core/src/sink/` | The delete walk and the Trash call, written once, over `Disk`. |
| `Command`, `RunId`, `Session`, `Action`, `Detail`, `Row`, `Line`, `Journal`, `LogStore`, `OpLog`, `LogWarning`, `ROTATE_AT` | `dustpan-core/src/oplog.rs` | Mole's grammar, written and parsed in one module. `OpLog` holds the rotation and warn-once policy over a two-call `LogStore`. |
| `History`, `Limit`, `SessionSummary`, `DeletionRecord` | `dustpan-core/src/history.rs` | Sessions and per-item records from log bytes. Never fails, never writes. |
| `Clock`, `WallTime`, `local_stamp`, `utc_offset`, `FixedClock` (testkit) | `dustpan-core/src/time.rs` | Core reads no clock. The wall time stamps rows, and the monotonic time ages evidence. |
| `LiveDisk`, `DirHandle` | `dustpan-sys/src/disk.rs` | `Disk` over owned descriptors and handles. Per-OS pieces live in `unix/delete.rs`, `{macos,linux,windows}/delete.rs` and `{macos,linux,windows}/trash.rs`. |
| `FileStore`, `SystemClock` | `dustpan-sys/src/{oplog,clock}.rs` | `LogStore` over a log folder opened with no link in any component. Opens are `O_APPEND`, `O_NOFOLLOW` and `O_NONBLOCK` and must find a regular file. Rotation runs under a lock that it waits for at most 1 s. `FileStore::read` is `history`'s non-blocking read, capped at 64 MiB. |
| `cmd/history.rs`, `Cmd::History` | `dustpan/src/` | `dustpan history [--json] [--limit N]`. Clap's `range(1..=200)` makes 0 and 201 usage errors (exit 2). |

### Data flow

`Executor::run(candidate)` goes cancel check, then `Recheck::approve`, then a mode branch, then one row.

1. **Cancel.** If the sticky `Cancel` is set, the result is `Outcome::Cancelled` with no probe and no row (SAFE-20).
2. **Recheck.** This is M0-3's recheck with three additions. First, the pass is seeded with the batch's process table and open-files index where each is younger than the lease. Otherwise the pass takes its own and the `Recheck` keeps it, aged from the moment before the check began. Second, after the second identity bind, a folder candidate is surveyed. `Probe::list` lists each folder link-free and returns its `FileId`, which must match what the parent's listing said. Every entry is judged by `Pass::descendant`: every layer but the bundle-id names (see "Descendant rows"), with the listing's `Node`, no rule owners, and leaf links not followed, because a link is removed as a link. The first kept entry keeps the whole candidate (`holds-kept`, with the inner reason). A dataless entry, a mount root, an entry on another volume, a folder that vanished or changed while listed, an unreadable folder, a tree deeper than 64 folders, more than 1,000,000 entries, or a survey longer than 30 s all keep it too; the deadline is checked before and after every listing and once more at the end. Third, when the live evidence is older than the lease once the survey ends, the recheck takes it again and judges the candidate against it (owners, its scope's owner, every file open at or below it), and asks again about each scope owner the survey met inside it. With `Lease::NONE` that happens after every folder's survey. A signal stops the recheck before the next listing or snapshot. Success returns `Approved { candidate, identity, manifest }`.
3. **Mode.** A signal that came during the recheck makes the outcome `Cancelled`, and no sink runs. `Preview` reports `WouldRemove(candidate.size)` or `WouldTrash(candidate.size)`. `Apply(disk)` dispatches on the rule's sink. The uninhabited `OwnerCommand` and `Privileged` arms are `match *never {}`.
4. **Row.** `sink::rows` maps the outcome to `REMOVED`, `TRASHED`, `SKIPPED` or `FAILED` with a `(sink=... size=... reason=... inner=...)` detail. A `TrashedOther` writes two rows: `FAILED` for the candidate, and `TRASHED` for the object that did move, under the path it had at the move, with `to=` naming where it landed. A would-be removal and a cancelled candidate get no row. A refusal gets a row in both modes, inside a `<command>-dry-run` session for a preview, as Mole does.

### The five questions

**How the Delete sink walks without following a swapped ancestor or crossing a device, and what it does with a protected path inside.** Protected paths inside are found before any removal, by the survey inside `recheck`, and they keep the whole candidate. That runs in preview too, so preview and apply refuse the same set. The sink then walks deepest first, relative to handles (`sink/delete.rs`):

- The parent opens with no link in any component: one `open` with `O_NOFOLLOW_ANY | O_DIRECTORY` on macOS, `openat` per component from `/` with `O_NOFOLLOW | O_DIRECTORY` on Linux (intermediate folders with `O_PATH`), and a per-component open that refuses a reparse point on Windows.
- Each folder below opens relative to its parent's handle with `O_NOFOLLOW | O_DIRECTORY`. Its listing entry must be on the candidate's volume, not a mount root (`statx` `STATX_ATTR_MOUNT_ROOT` on Linux catches a bind mount), and in the `Manifest`, and its `fstat` must match the listing. A folder that fails is left with everything in it, and the item ends `Failed(changed-during-removal)` with the bytes already freed.
- Files in a checked folder are unlinked by name. Folders go with `AT_REMOVEDIR`, which removes only an empty folder, so a folder swapped in under a name at that moment loses nothing.
- A mount that appears after approval has a `FileId` the survey never judged, so the walk does not enter it.
- A missing target is `Gone` at the parent open, at the target open, or at the final `rmdir`.
- The walk holds one handle per level. The survey refuses a tree deeper than `MAX_DEPTH` (64) before anything is removed, so the cap is never met halfway through a delete; a folder moved deeper after the survey is left, as above.

**How identity is rechecked at removal and how much of the chain it binds.** The parent is opened with no link in any component, and its `fstat` must equal the approved parent `FileId`. That binds the whole ancestor chain at one instant: the path resolved, link-free, to the very folder discovery recorded. From then on the descriptor is bound to that folder object, wherever it moves. A folder target is then opened relative to it and its `fstat` must equal the approved target. A file target gets `fstatat` and then `unlinkat` relative to the same descriptor. The window between those two calls is the narrowest that unlink-by-name allows. Ancestors above the parent are bound by "no link and same parent object", not by their own ids. SAFE-10 asks for parent and target, and a different ancestor object would resolve to a different parent. Any mismatch is `Kept(identity-changed)` with nothing touched. The M4 helper's SAFE-14 checks need a descriptor per ancestor, which the Linux per-component open already has and macOS will add then.

**How thousands of candidates are rechecked without one snapshot each, still meeting SAFE-10.** Through `Recheck` with `Lease::BATCH`. Identity, spelling, every policy layer and the survey stay per candidate. Only the process table and the open-files index are shared, and only while each is under 1 s old by the monotonic clock, measured from before it was asked for. The age is checked when a recheck starts and again when its survey ends, so no approval rests on evidence older than one lease, the survey's own time included. A long sink call ages it out, so the next candidate takes fresh evidence. All evidence is taken by the sink-time `Recheck`. Discovery's passes are dropped and its identity is only compared, never refreshed. A per-candidate snapshot does not close the race either, because an owner can start while a multi-second delete runs. The lease adds at most 1 s to that window. See "Note on SAFE-10" for the measured cost.

**Where the row model lives and how a crash leaves a readable log.** Core's `oplog.rs` owns the row model and the grammar, both writing (`format`) and reading (`parse`). It also owns the rotation and warn-once policy (`OpLog<S: LogStore, W>`), so `tests/oplog.rs` runs without a file system. Sys's `FileStore` has two calls: rotate above a limit, and one `O_APPEND` write per line. Rotation takes an advisory lock on `operations.log.lock` and reads the size again under it, so two runs that start together rotate once. No log call can block a run: every open is non-blocking and must find a regular file, and a lock held past 1 s is a warning, not a wait. The log is append-only and every line names its run, so concurrent runs and the helper's rows need no lock to append. Rows are written synchronously, one per item, after the outcome is known. A crash leaves at most one torn last line and no end marker. Each start marker begins with a newline, so a torn line stays a line of its own, which the reader skips. `history` shows the crashed session as not ended. The next run's recheck finds removed items `Gone`, so rerunning a plan converges.

**Which outcomes and counters the executor returns.** `Outcome` has `WouldRemove(Size)`, `WouldTrash(Size)`, `Removed(Size)`, `Trashed(Size)`, `TrashedOther(Moved { from, to })`, `Gone`, `Kept(KeptReason)`, `Failed(Failure { cause, at, freed })` and `Cancelled`. `TrashedOther` counts as failed. Preview yields the first two plus `Gone`, `Kept` and `Cancelled`. Apply yields the rest. `Tally` keeps `freed` (fed only by `Removed` and a failed delete's partial bytes), `moved_to_trash` (fed only by `Trashed`) and `would_free` in separate fields, so no code path adds Trash bytes to freed (SAFE-21). `Removed` reports bytes actually freed. The walk counts each file's allocated size when its last link inside the tree goes, so a hard link that also lives elsewhere frees nothing.

### Invariants encoded in types and structure

- A sink takes `Approved` by value, and only `Recheck::approve` builds one. The `Approved` carries the `Manifest`, so "approved" means the candidate and everything it holds.
- `Mode::Preview` holds no disk, so no preview can reach a sink (SAFE-18 by construction).
- `Disk` methods need a `Seal`. The binary can hold a `LiveDisk` and still cannot remove a file except through `Executor` in apply mode (SAFE-01).
- The workspace `clippy.toml` bans `libc::unlink`, `unlinkat`, `rmdir`, `rename`, `renameat`, `renameat2`, `renamex_np`, `renameatx_np`, `NSFileManager`'s trash and remove calls, the `windows-sys` delete, move and disposition calls (`NtDeleteFile`, `NtSetInformationFile` and their `Zw` forms among them), `SHFileOperationW`, and `IFileOperation`'s delete, move and rename calls, with `allow-invalid` for other OSes' paths. `crates/dustpan/tests/seams.rs` lists the files that may lift the ban.
- The Trash sink has no path to `Disk::unlink`, and a Trash failure maps only to `Failed` (SAFE-12).
- `Session` is built only by the executor, from its own mode, so a preview can never log as a real run.
- `Journal::write` returns nothing. A log failure cannot stop a delete (SAFE-22). `OpLog` calls its warning callback at most once.
- The survey's depth bound is the walk's handle cap (one constant).

### Validation at the boundaries

`AbsPath` and `Name` stay the only path parsers for input. A name a directory read returns may hold a control byte, which `Name::parse` refuses; the survey and the walk join it with `AbsPath::join_listed`, judge it like any other name, and every printed form escapes it, so one hostile file name cannot keep a cache forever. Sys classifies errno once (`unix/delete.rs::classify`) into `DiskError`, and core decides what each class means. `history` treats the log as untrusted bytes: it skips lines that do not parse, decodes with U+FFFD, and escapes for text output.

### What it deliberately does not do

- It writes no `deletions.log`. Each item gets one row, once. `deletions[]` is derived from the item rows, and `logs.deletions` names the operations log.
- It writes no row for a would-be removal or a cancelled candidate.
- It shares no path observation between candidates, only live evidence.
- It removes no folder minus a kept child. That would be I-37's generic parent sweep.
- It has no removal deadline yet. The walk checks `Cancel` between entries, and M3's section budgets bring the SAFE-20 removal timeout.

## Synthesis decision (M0-5 arena)

### Base

Candidate 1. The root and the cross-judge picked it independently (judge: c1 26, c2 22, c3 19, c4 19 of 30). It is the only candidate that judges a folder's contents inside `recheck`, before an `Approved` exists, so preview and apply refuse the same set (SAFE-18). It is the only one with real, tested code behind the evidence lease, the row grammar and `history`, and the only one that measured the `rm -rf` baseline. Its delete walk is written once in core over a sealed, handle-relative `Disk` trait, and preview holds no disk.

### Fork rulings

- Batch recheck. Share live evidence for a short window; never share identity, spelling or policy. The lease satisfies SAFE-10 only as built here: each snapshot's age starts from the monotonic time read before its probe call and is checked at every recheck, the snapshot is taken inside the executor after review, identity is bound fresh per candidate and never refreshed. The lease covers the process table and the open-files snapshot together. Window: 1 s by default, confirmed by measuring the overhead on the bench Mac.
- A kept, held, mounted, unreadable or online-only entry inside a folder keeps the whole folder, refused before mutation (SAFE-27, SAFE-18). Deleting around it would show one size on the review screen and remove a different set.

### Grafts

1. From candidate 2: serialize log rotation with an advisory lock so two runs cannot clobber `operations.log.old`.
2. From candidate 3: judge a listed name that fails `Name::parse` (a control byte) as a raw `OsStr` instead of failing the listing; and a named zero lease (`Lease::NONE`) for the M4 helper and tests.
3. From candidate 4: survey bounds (entries, depth, deadline) that refuse a candidate up front (SAFE-20), so the walk's handle cap can never be hit mid-removal; and `O_NOFOLLOW` on the log file open.

### Rejected grafts

- Candidate 3's write-ahead `REMOVING` rows: a second row per item that can disagree with the first after a crash. A crashed session reads as not ended, and rerunning converges because a gone path is success.
- Candidate 4's per-entry allowlist with a 500,000-entry cap: the folder manifest (16 bytes per folder) gives the same "never enter an unjudged folder" property.
- Candidate 2's deferred per-component `openat` ancestor chain: it belongs to the M4 helper's SAFE-14 checks, recorded there as the path forward, not built now.

### Decisions applied by default (operator reviews at the end)

- `history --json` prints Mole's HIST-02 object with Mole's keys (porting note 9), plus two additive keys, `schema_version` and `command`, so PLAN's envelope rule holds where it does not break Mole's scripts. No `items[]`.
- Row stamps keep Mole's grammar. The session start marker carries the local UTC offset after Mole's stamp, and `history` adds it to `deletions[].timestamp` (Mole's `%z`). Mole's reader keeps the whole tail as the start time, so `mo history` shows the offset in `started_at` for a session dustpan wrote.
- One log file. `deletions[]` is derived from the item rows; `logs.deletions` names the operations log.
- A file created after the survey inside a judged folder is removed as `rm -rf` would. Accepted residual: candidate trees are rebuildable data, every folder entered was judged, and the open-files evidence was checked at recheck.
- The sink bench is the first implementation step, so the 2x `rm -rf` budget is measured before the walk is tuned.
- Before merge, the survey runs over the real `~/Library/Caches` read-only, and candidates kept only by a descendant row are counted (see "Descendant rows").

## Note on SAFE-10

SAFE-10 says to re-run the owner probe after slow work. This design re-runs it at the sink for every candidate, but answers from a snapshot up to 1 s old when the batch already took one. The age counts the survey: evidence that a survey aged past the lease is taken again before the approval, and with `Lease::NONE` every folder takes it again after its survey, so the M4 helper pays two snapshots per folder. Measured on the bench Mac (`docs/perf.md`, M0-5 section): one process table plus one open-files snapshot costs 90.4 ms (median of 5). Rechecking 200 small cache folders in one preview took 108.79 ms per candidate with `Lease::NONE` and 0.73 ms with `Lease::BATCH`. A 1 s lease spends at most 9% of each second on snapshots. A 250 ms lease would spend 36%, so the window stays 1 s.

## Descendant rows

Before merge, the survey ran read-only over the real `~/Library/Caches` of the bench Mac (`cargo run --release -p dustpan-sys --example survey_caches -- ~/Library/Caches`). It counts the candidates that every layer passes at discovery and that the survey then keeps only because of a row on a name inside. The synthesis set the bar at a handful (6).

| Survey judges the contents with | Entries | Eligible at discovery | Would be removed | Kept only by a row inside |
|---|---|---|---|---|
| every layer (first build) | 244 | 54 | 42 | 12 |
| every layer but the bundle-id names (merged) | 244 | 54 | 52 | 2 |

Eleven of the 12 were bundle-id name patterns that matched a cache's own contents: `org.sparkle-project.Sparkle` inside four app caches, `WebKit/NetworkCache` inside three, two header names containing `loongarch` (CocoaPods, node-gyp), `BluetoothPeripheral` in a Yarn package and `networkimage.dia` in SwiftPM's cache. A bundle id names an app's own folder, and Mole checks it only on the item it removes. So `Pass::contents`, the pass that judges a candidate's contents, steps the policy with `Bundles::Ignored`. Inside a candidate, every row anchored on a path (critical, protected and whitelisted), the keyword names, the live checks and the shape checks still apply. `a_folder_named_for_an_app_inside_is_part_of_the_cache` and `a_protected_row_deep_inside_keeps_the_folder` in `crates/dustpan-core/tests/recheck.rs` pin both sides.

The two left are kept on purpose for now:

- `Mozilla`: a floating row protects `ms-playwright` (installed browser binaries) wherever it appears, and one sat under `Mozilla/updates/`.
- `Yarn`: the safety list's `.DS_Store` entry matched a `.DS_Store` inside a package. Mole's entry spares those files from its `.DS_Store` sweep, so a cache folder someone opened in Finder is kept. Whether whitelist name entries should apply inside a candidate is a separate decision for the operator.

## Tradeoffs accepted

- We accept live evidence up to 1 s old at recheck in exchange for one snapshot per second of run instead of one per candidate. The window it adds is at most 1 s on top of the sink's own duration, which a per-candidate snapshot leaves open anyway.
- We accept refusing a whole folder when anything inside is kept (a mount, a protected path, an unreadable subfolder, an online-only file) in exchange for never removing a folder minus a child. A cache with a mounted image inside is not cleaned until it is ejected.
- We accept a full read-only listing of every candidate tree in preview in exchange for preview refusing exactly what apply refuses.
- We accept that files created after the survey inside a judged folder are removed, as `rm -rf` would, in exchange for a manifest of folders only.
- We accept writing the row after the sink in exchange for one row per item. A crash during a sink call leaves that item without a row and the session without an end marker.
- We accept deriving `deletions[]` from operations rows in exchange for one record per deletion and no second file to keep in sync.
- We accept a `&Seal` argument on every `Disk` method in exchange for no removal call reachable outside core's sinks.
- We accept that a delete stopped by permission, a new entry or a signal leaves the rest removed and reports `Failed` with the bytes freed. Delete targets are rebuildable data. Trash moves are whole or nothing.
- We accept that `st_blocks` counts an APFS clone's shared blocks as freed. Sizing has the same limit (`du`).

## Alternatives considered

- **Survey inside the sink.** The sink signature stays `Approved` only. But preview never calls the sink, so it would print `WouldRemove` for a folder the real run refuses: the dry-run parity defect Mole shipped twice (`safety-invariants.md` 4.1).
- **A `Surveyed` stage after `Approved`, with the sink taking it.** The parity is the same, but it breaks "a sink takes `Approved` and nothing else" and adds a stage type that repeats the path and identity.
- **The walk per OS in sys, with no `Disk` trait in core.** It costs three walks to keep in step. A mount, a swap or a link replacing a parent can then only be tested with real mounts and races, and `unlinkat` stays callable from anywhere in sys.
- **Per-candidate snapshots (M0-3's behavior).** Simplest, and SAFE-10 is met literally. It costs a snapshot per candidate, and the race it closes stays open for the sink's own duration. `Lease::NONE` keeps it available.
- **An incremental process table.** Fresher than a lease, but it needs exec detection per OS and makes the probe stateful. It can replace the lease later behind the same `Recheck` seam.
- **Two logs, as Mole keeps.** Each deletion would write two rows that can disagree after a crash between them.
- **`execute(plan) -> Vec<(Candidate, Outcome)>` as one call.** A smaller surface, but the TUI cannot render progress, cancel cannot be observed per item, and the M4 helper cannot interleave.

## Implementation reconciliation

What changed from the candidate 1 sketch, and why:

- The sketch was written against an older guard. Here `Live` holds an `Arc` process table and an `Arc<OpenIndex>`, and `Evidence` shares both; each part keeps its own age.
- `EVIDENCE_TTL` became `Lease` with `BATCH` and `NONE` (graft 2), and `Services` names the lease, so each caller decides it. Freshness is strict (`age < window`), so `Lease::NONE` never shares even under a frozen test clock.
- `DirEntry.name` is a raw `OsString`, and `Disk` methods take `&OsStr` (graft 2). `AbsPath::join_listed` is the one place such a name joins a path.
- The survey has bounds (graft 3): 64 folders deep, the walk's handle cap, so the sketch's 64-handle TODO is gone; 1,000,000 entries; 30 s.
- `LogStore` is `rotate_above(limit)` and `append`. Rotation locks a separate `operations.log.lock` file, so it needs no check that the locked file is still the one at the path. The rotation `rename` is the one ban lift outside a sink file; it renames dustpan's own log (SAFE-01's exception for files the process created) and carries a `SAFE:` note, and the seams test lists it.
- Linux opens by path one component at a time with `O_NOFOLLOW` (no `openat2`), which works on every kernel, and reads `statx` for mount roots instead of `RESOLVE_NO_XDEV`.
- macOS lists with `readdir` plus `fstatat` instead of `getattrlistbulk`, and turns off dataless materialization per thread with `setiopolicy_np`. The bench measures the cost.
- Issue 5 landed first: `GuardInputs.home` is `homes: Homes`, the `HOME` folder plus the account's home from the user database.
- Issue 11 landed first: the whitelist fold key uses full case folding.
- The survey leaves bundle-id names out (`Bundles::Ignored`), after the real `~/Library/Caches` count (see "Descendant rows").
- Linux Trash is the freedesktop Trash, with Mole's `internal/xdgtrash` rules: the home Trash for an item on the home's device, else `$topdir/.Trash-$uid` on the item's own volume. The `.trashinfo` is reserved with `O_EXCL` before the move. The move is `renameat2` with `RENAME_NOREPLACE` between open folders, so it never replaces an entry or crosses a device. A failed move removes the reserved info and leaves the item where it is.
- Windows Delete opens every name with `NtCreateFile` relative to a handle already checked, with `FILE_OPEN_REPARSE_POINT`, from `\??\C:\` down, and refuses a reparse point on the way as `NotAFolder`. A junction is removed with `Unlink::NotDir`, as the link it is. A folder goes only if it has no reparse point. The delete is `FileDispositionInfoEx` with POSIX semantics on the handle whose attributes were just checked, falling back to the classic disposition on FAT, exFAT and older ReFS.
- macOS Trash moved off `NSFileManager trashItemAtURL` after interrogate round 1 (A2-F2): it moves by path, so a parent swapped for a link while the sink ran sent a folder the guard never approved to the Trash (2 of 800 attempts in the reviewer's run; 532 of 2000 in `trash_race_lane` on a scratch APFS image, which swaps the parent at points spread over the sink's own duration). The sink now makes one `renameatx_np(RENAME_EXCL | RENAME_NOFOLLOW_ANY)` from the checked parent's handle into `.Trash` of the account's home, or `.Trashes/<uid>` on another volume, after checking that the Trash is this user's folder that nobody else can write (SAFE-13). A link anywhere on the way, a Trash on another device, or a full set of names is `trash-failed`. Privacy protection forbids opening `~/.Trash`, so the move and the check after it go by name from the home's handle. With it, the same lane moved the victim in 0 of 2000 attempts. What it costs: Finder keeps no Put Back record for the item, and dustpan picks the name in the Trash (`name`, `name 2`, ...). S2's verdict changes accordingly (`docs/decisions.md`).
- A Trash move that takes another object than the approved one (the candidate replaced in its folder between the check and the move) is `TrashedOther`, with the path the moved object had and where it landed, both logged.
- A signal during a recheck stops it before its next listing or snapshot, and no sink runs after it (SAFE-20); the Trash sink checks once more right before its move, and the delete walk before its last `rmdir`.
- Windows Trash is `IFileOperation` with `FOFX_RECYCLEONDELETE`, through the `windows` crate's generated COM bindings rather than hand-written vtables. A progress sink returns `E_ABORT` from `PreDeleteItem` unless the shell will recycle (SAFE-12), and `PostDeleteItem` records where the item landed. Only a fixed drive is tried. None of this has run outside the Windows CI runners.

## Open questions and risks

- `O_NOFOLLOW_ANY` needs macOS 11.
- The survey's entry bound has no unit test; building a million-entry fake tree in a test costs more than it proves. The deadline has one.
- Windows: the guard keeps every path on Windows until M8 lists open files (`file-open-unknown`), so no Windows candidate reaches a sink in M0. The Windows sinks are built and unit-tested on the Windows CI runners; live lane 9 is operator-run.
- Windows Trash: the case SAFE-12 exists for, a bin that refuses an item so that the shell would delete it for good, cannot be produced on CI without changing the user's registry. Only the progress sink's answer is tested. The classic-disposition fallback never runs on CI, whose disks are NTFS.
