# Mole `mo analyze` and `mo status` TUI inventory (macOS, main @ 5dd93fe3)

Path prefixes used in every `Source` cell: `A/` = `cmd/analyze/`, `S/` = `cmd/status/`, `U/` = `internal/units/`, `repo:` = repo root. All line numbers refer to the checked-out tree at `../mole`. Both programs are Go, darwin-only (`//go:build darwin`, non-darwin stub prints "analyze is only supported on macOS" and exits 1: A/main_stub.go:1-13).

## 0. Launch path and build (shared)

- `mo analyze` / `mo analyse` and `mo status` are dispatched by the bash entry script to `bin/analyze.sh` / `bin/status.sh`, passing remaining args (repo:mole:335-340). `--debug` is stripped by the bash wrapper before dispatch and sets `MO_DEBUG=1` (repo:mole:77-93, 97-98). Interactive menu entries 4 (Analyze) and 5 (Status) launch the same scripts (repo:mole:267-268, 281-284).
- `bin/analyze.sh` / `bin/status.sh` are 15-line shims: `exec "$SCRIPT_DIR/analyze-go"` (or `status-go`) with `"$@"`; if the binary is not executable they print "Bundled ... binary not found. Please reinstall Mole or run mo update" to stderr and exit 1 (repo:bin/analyze.sh:7-15, repo:bin/status.sh:7-15).
- `run_mole_command` execs directly (no update-check notice) when any of stdin/stdout/stderr is not a TTY, or when args contain `--json`, `-json`, `--json=*`, `--ndjson`, `--watch`, `-watch`, `--help`, `-h`, `--version`, `-V`, `--list`; otherwise it runs `check_for_updates`, then runs the child under signal-forwarding traps (repo:lib/manage/update.sh:1030-1075).
- Build: `go build -ldflags="-s -w" -o bin/{analyze,status}-go ./cmd/{analyze,status}` (repo:Makefile:13-17, 35-37). Release builds are `CGO_ENABLED=0 GOOS=darwin GOARCH={amd64,arm64}` "pure-Go so the macOS SDK on the release runner cannot raise the Mach-O minimum OS version via cgo" (repo:Makefile:60-69). CI tars `analyze-darwin-{arm64,amd64}` + `status-darwin-*` (repo:.github/workflows/release.yml:56-59); `install.sh` downloads the prebuilt `analyze` and `status` binaries (repo:install.sh:1296-1297, 1625). Porting consequence: no cgo; every macOS API is reached through shell-outs, sysctl via x/sys or gopsutil's purego, or raw syscalls (A/clone_darwin.go:47).
- Analyze reads the optional env `MO_ANALYZE_PATH` (A/main.go:71, 89-93), `MO_NO_OPLOG=1` (A/oplog.go:43), `MOLE_DELETE_LOG` (A/oplog.go:63-65), `HOME` (many).

## 1. Feature table

Privilege column: "user" = runs as the invoking user with no sudo; analyze never elevates. "FDA-limited" = results are partial without Full Disk Access (permission failures are classified, A/model.go:60-66).

| ID | Command | Flags/options | Behavior | Privilege | Safety rule | Source |
|---|---|---|---|---|---|---|
| ANALYZE-01 | `mo analyze` | no args | Machine-wide **overview**: fixed list of rows (Home, User Library, Applications, System Library, mounted volumes, "insight" rows) sized one by one by bounded `du` workers, cached 7d | user, FDA-limited | read-only; skips iCloud/CloudStorage in `du` | A/main.go:97-98, 187-250; A/constants.go:68-78 |
| ANALYZE-02 | `mo analyze [PATH]` | positional `PATH` (first arg only; `filepath.Abs`, relative OK, `~` is not expanded by the program) | Live drill-down scan of one directory in the TUI; also prefetches overview sizes in background for 30s | user | read-only | A/main.go:89-106, 108-118 |
| ANALYZE-03 | `mo analyze` | env `MO_ANALYZE_PATH=<dir>` | Env wins over positional arg | user | read-only | A/main.go:89-93 |
| ANALYZE-04 | `mo analyze --json [PATH]` | `--json` (Go `flag` also accepts `-json`, `--json=true`) | Non-interactive scan; pretty-printed JSON to stdout (schema in section 2.10). Overview JSON when no PATH | user | read-only | A/main.go:22, 78-79; A/json.go:15-58 |
| ANALYZE-05 | `mo analyze -h` / `--help` / `-help` | help | Prints usage text to stdout, exit 0; unknown flag: message + "Use 'mo analyze --help'" to stderr, exit 1 | n/a | n/a | A/main.go:28-64; A/usage_test.go:24-40 |
| ANALYZE-06 | in-TUI: drill down | Enter/Right/l | Push history, scan child dir (cache reuse) | user | refuses nothing; read-only | A/update.go:761-765, 1284-1367 |
| ANALYZE-07 | in-TUI: back / overview | Esc, b, Left, h | Pop history; at top of a path scan, switch to overview; in overview quit | user | read-only | A/update.go:720-737, 766-772, 1175-1182 |
| ANALYZE-08 | in-TUI: refresh | r / R | Invalidate cache for this path and its direct child dirs, rescan bypassing cache; in overview, invalidates every row and re-measures | user | read-only | A/update.go:773-819 |
| ANALYZE-09 | in-TUI: Top files (large-file finder) | t / T | Toggle top-20 largest files list for the current directory tree; only after scan finishes; not in overview | user | skips source/data extensions (`.go .js .json .db ...`) and folded dirs | A/update.go:820-837; A/constants.go:10, 254-296 |
| ANALYZE-10 | in-TUI: incremental filter | `/` then type | Case-insensitive substring filter over name or `~`-path on dir view or Top-files; disabled in overview | user | any query change clears multi-selection so an action cannot hit a hidden row | A/update.go:838-850, 1087-1173; A/model.go:543-565 |
| ANALYZE-11 | in-TUI: live sort mode | s / S (only while scanning, not overview) | Toggle `freeze-on-move` (default) vs `continuous` re-sorting of rows while the live scan streams | user | n/a | A/update.go:851-859; A/live_config.go:5-17 |
| ANALYZE-12 | in-TUI: open | o / O | `open <path>` on selection (multi-select aware, max 20) | user | `validatePath` (absolute, no NUL, no `..`) | A/update.go:860-905; A/main.go:252-263 |
| ANALYZE-13 | in-TUI: reveal in Finder | f / F | `open -R <path>` (multi-select aware, max 20) | user | same | A/update.go:906-951 |
| ANALYZE-14 | in-TUI: Quick Look | p / P | `open <path>` with default app; single file only (dirs ignored in dir view) | user | same | A/update.go:952-970; A/main.go:265-273 |
| ANALYZE-15 | in-TUI: multi-select | Space | Toggle selection by **path** key; blocked during scan; cannot select `scanUnavailable` rows; overview not selectable | user | selection keyed by path not index | A/update.go:971-1025 |
| ANALYZE-16 | in-TUI: move to Trash | Delete / Backspace, then Enter to confirm | Moves selected (or highlighted) path(s) to Trash (never `rm`); blocked during scan; refuses rows with unknown size | user | protected-path list, home head folders, iCloud/Docker/OrbStack/EDR protections, audit log (section 2.8) | A/update.go:1026-1082, 651-707; A/delete.go:138-162, 309-632 |
| ANALYZE-17 | in-TUI: quit | q / Q / Ctrl+C | Quit; stops du children (waits up to 500ms) | user | kills in-flight `du` | A/update.go:718-719; A/update.go:1387-1394; A/constants.go:22 |
| ANALYZE-18 | hint: cleanable marker | automatic | 🧹 on directory rows that are CACHEDIR.TAG caches or known project dependency/build dirs and not handled by `mo clean` | n/a | exclusion list of paths `mo clean` owns | A/cleanable.go:17-144; A/view.go:562-570 |
| ANALYZE-19 | hint: stale marker | automatic | Gray `>3mo`..`>Nyr` suffix when atime is 90+ days old | n/a | n/a | A/format.go:172-197; A/view.go:566-569 |
| ANALYZE-20 | hint: overview insights | automatic | "Hidden space" overview rows (iOS Backups, Old Downloads (90d+), logs, dev caches) shown only if the path exists; empty ones hidden | user | read-only | A/insights.go:15-92 |
| ANALYZE-21 | hint: local APFS snapshots | automatic in overview | `tmutil listlocalsnapshots /` count banner ("N local snapshots · snapshot-only space is not listed below") | user | sizes intentionally not inferred | A/snapshots.go:15-77; A/view.go:32-43 |
| ANALYZE-22 | uninstall hint on delete | automatic | When the delete target is a `.app` bundle, warns only the bundle moves and suggests `mo uninstall <App>` | n/a | shell-quotes the name, falls back to `<App>` on odd names | A/view.go:453-459, 477-518 |

---

## 2. Analyze internals

### 2.1 Entry, modes, startup

- `main()` → `parseArgs` → `resolveScanTarget(env, args)` → `go pruneAnalyzerCache()` → JSON or TUI (A/main.go:66-83). No target = overview anchored at `"/"`, flagged `isOverview` (A/main.go:97-99). `m.inOverviewMode()` is `isOverview && path=="/"` (A/model.go:283-285). Note: `mo analyze /` is a real directory scan of `/`, not the overview.
- TUI: Bubble Tea `tea.NewProgram(m, tea.WithAltScreen())`; no mouse (A/main.go:120). On exit `stopMeasurementsForExit` cancels live/overview/background-cache work and polls `activeDu` up to 500ms so no `du` is orphaned (A/main.go:125; A/update.go:1387-1394).
- Free-space label: `statfs(path)` → `Bavail*Bsize`, shown only in overview header (A/main.go:136-140; A/view.go:27-31).
- Overview init: hydrates rows from cache, then schedules scans; `Init()` returns an `initializeMsg` rather than starting work directly because Bubble Tea passes `Init` a model copy (A/update.go:78-84, 350-357).
- Non-overview start: `scanCmd` (A/update.go:98-131) tries (1) fresh disk cache → shown immediately; if `NeedsRefresh`, flagged stale; (2) stale cache (<3d) → painted at once then fresh rescan; (3) otherwise live scan. A prior-scan file count is peeked from cache even when stale, to drive a percent progress estimate capped at 99% while scanning (A/main.go:177-182; A/view.go:96-108).

### 2.2 Scanning model

- **Two scanners.** The TUI uses the *live scan* (streaming) in A/live_scan.go; `--json` uses the blocking `scanPathConcurrentAllEntries` in A/scanner.go:259-264, 274-647. Both share the same size primitives.
- **Live scan** (A/live_scan.go): `readLiveScanInitialEntries` does one `os.ReadDir(root)`; files and symlinks are sized inline, directories become rows with `Size=-1` ("scanning") and queued targets of kind directory / folded directory / HomeLibrary (A/live_scan.go:139-257). `runLiveScan` runs each target (via `entrySem` pool or inline), emits `liveScanChildProgress` (lossy, every 200ms when bytes changed, A/live_scan.go:417), `liveScanChildDone`, `liveScanFailed`, and a final `liveScanComplete` (A/live_scan.go:259-404, 406-463). Event channel capacity is `targets*4` with `targets+1` slots reserved so progress can never starve completion (A/live_scan.go:40-79).
- **Scan identity/cancellation.** Every request reserves a monotonically increasing `liveScanID` and a `scanPublication` (ctx + mutex-guarded commit/cancel) before dispatch; stale/superseded results are dropped by id+path match (A/update.go:86-96, 436-440, 495-501; A/scanner.go:77-130; A/live_scan.go:21).
- **Concurrency budgets** (`scanLimiter`, four independent semaphores + one sync.Map) (A/scanner.go:132-177):
  - `entrySem` workers = `clamp(NumCPU*1, 2, 12, childCount)`; `tryAcquireEntry` non-blocking, else the caller scans inline (A/scanner.go:166-170, 179-189). Constants `minWorkers=2, maxWorkers=12, cpuMultiplier=1` are deliberately low because blocked syscalls each hold an OS thread and macOS aborts the process with "failed to create new OS thread" past the per-user limit (A/constants.go:53-62).
  - `duSem` = `min(4, NumCPU)` concurrent `du` processes; `duQueueSem` = 2x that bounds queued goroutines (A/scanner.go:143-154, 173-174).
  - `fastSem` = `min(NumCPU, 12)` for the fallback walker (A/scanner.go:156-159, 175).
  - Overview: `maxConcurrentOverview = 8` row measurements (A/constants.go:23; A/update.go:20).
- **Per-directory sizing.** Non-folded subdirectories recurse via `scanPathConcurrentWithLimiter` (readdir walk, children classified, heaps for top-N). "Folded" directories (`foldDirs`: node_modules, .git, Caches, DerivedData, target, vendor, build, .npm internals, etc.; ~150 names) are **not expanded**: they are sized with one `du` call and shown as a single row, falling back to a parallel readdir walker if `du` returned nothing (A/scanner.go:481-517, 724-741, 748-830; A/constants.go:80-212).
- **`du` invocation.** `/usr/bin/du -skPx [-I name ...] <path>`: KiB blocks, POSIX output, `-x` stay on one device. Bytes = KiB*1024 (A/scanner.go:1053, 1095). Hard-coded absolute path because BSD-only `-I` and error text parsing break under GNU coreutils (A/scanner.go:41-44). Exit status 1 with only "Permission denied"/"Operation not permitted" lines is reclassified as a *permission* partial (A/scanner.go:1069-1073, 1125-1141). Non-zero exit still returns the bytes read (partial) and does **not** trigger another walk (A/scanner.go:1090-1094).
- **Timeouts.** `duTimeout=30s` per du while listing a folder (also per row in `--json` overview); `overviewDuTimeout=5min` per overview row (no fallback, spinner meanwhile); fast walker 5min; Spotlight `mdfind` 5s (`mdlsTimeout`); `open` 10s; `trash` 30s; local snapshot probe 3s; UI tick 100ms; scan send timeout 100ms (A/scanner.go:51-55, 754; A/constants.go:21,63-65; A/delete.go:23; A/snapshots.go:15).
- **Large-file collection.** During the walk, files with `size >= largeFileMinSize` (starts at 1 MiB; becomes the heap minimum once 20 are held) are sent to a min-heap of 20; extension denylist `skipExtensions` removes source/data files (A/scanner.go:322-364, 579-585; A/constants.go:10,16,254-296). When `useSpotlight` (JSON path only; live scan passes false for subtrees) it also runs `mdfind -onlyin <root> "kMDItemFSSize >= 100MiB"`, filters (extensions, folded dirs, symlinks, dirs, other devices) and **replaces** the walk result only if it has more entries (A/scanner.go:622-631, 833-917; A/constants.go:15).
- **File size semantics.** Per file: `min(st_blocks*512, st_size)` (allocated size, so sparse/cloud placeholders report their on-disk use) (A/scanner.go:1328-1339). Symlinks count their own link size, shown as `name →`, not followed (A/scanner.go:378-403; A/live_scan.go:165-187).
- **Hardlinks.** `seen` sync.Map keyed `(dev, ino)` for files with `nlink>1`; second sighting counts 0 and marks the subtree `dedupedHardlink`, which is scan-order dependent and therefore **never written to the disk cache** (A/scanner.go:1302-1316; A/model.go:102-106; A/scanner.go:705-712). The User Library overview row is a single serial `du` so cross-directory hardlinks dedupe like `du` does (A/scanner.go:1121-1122; .claude/rules/go-tuis.md Analyze bullet).
- **APFS clones.** `getattrlist(ATTR_CMNEXT_CLONEID|EXT_FLAGS, FSOPT_ATTR_CMN_EXTENDED)` via raw `SYS_GETATTRLIST`; clone id goes in the *fork* attr group (putting it in commonattr silently returns different data). A file with `EF_SHARES_ALL_BLOCKS` is counted once per `(dev, cloneID)`; an edited clone counts in full because macOS does not say how many blocks are shared (A/clone_darwin.go:12-55; A/scanner.go:1287-1326).
- **Cross-mount.** Scans behave like `du -x`: a child directory on another device (`st_dev` differs from the scan root) becomes a browsable row with `IsMount=true`, size 0, state complete, not counted in the parent and not queued as its own target; Spotlight candidates on other devices are dropped; `measureOldDownloads` skips them (A/scanner.go:418-429, 791-793, 887-889, 1351-1392; A/live_scan.go:197-209; A/insights.go:143-147).
- **Skip lists.** At `/`: `skipSystemDirs` (dev, tmp, private, cores, net, home, System, sbin, bin, etc, var, Volumes, Network, .vol, .Spotlight-V100, .fseventsd, .DocumentRevisions-V100, .TemporaryItems, .MobileBackups; `opt` and `usr` explicitly false) (A/constants.go:214-236; A/scanner.go:412-414). Everywhere: `defaultSkipDirs` (nfs, PHD, Permissions, OrbStack, Colima, VMware Fusion, VirtualBox VMs, Rancher Desktop, .lima, .colima, .orbstack) (A/constants.go:238-252; A/scanner.go:407-409).
- **Overview-only `du -I` ignores:** `Mobile Documents` (iCloud Drive FileProvider can block du for tens of seconds) and `CloudStorage` (Dropbox/GDrive/OneDrive File Provider domains) (A/constants.go:68-78; A/scanner.go:1156-1170). When sizing `~`, `~/Library` is excluded and measured as its own row to avoid double counting (A/scanner.go:950-955, 1172-1248; A/main.go:195-206).
- **Fallbacks.** If `du` fails with zero bytes, overview sizing falls back to a logical-size `WalkDir` using the same exclusions (A/scanner.go:961-963, 1250-1285).
- **Scan state model.** `scanState` = complete / partial / unavailable (JSON strings), zero value complete. `measurementState(size, err)`: no err = complete; err with size>0 = partial; err with 0 = unavailable. Permission-only partials persist like complete results; transient partials (timeout, cancel, vanished, I/O) never persist and always refresh (A/model.go:17-70, 114-117; A/scanner.go:30-75).
- **Volumes.** Overview adds local, writable, browsable disks mounted directly under `/Volumes`: `getfsstat(MNT_NOWAIT)` (cached kernel table, never wakes network mounts), requires `MNT_LOCAL`, not `MNT_RDONLY|MNT_DONTBROWSE`, device prefix `/dev/disk`, `Dir(path)=="/Volumes"`, cleaned path, deduped (A/volumes.go:12-44; A/main.go:209).
- **APFS snapshots.** Only counted (not sized): `/usr/bin/tmutil listlocalsnapshots /` parsed with `^[A-Za-z0-9_-]+(\.[A-Za-z0-9_-]+)+$`; counts Time Machine names and non-`com.apple.` third-party backup snapshots, excludes other `com.apple.*` (A/snapshots.go:42-77). Failed re-probe shows "· last successful check" (A/view.go:37-40).
- **Sorting.** Always size descending, ties broken by path ascending, via one comparator (`sizePathBefore`) shared by heaps, final sort, live display, `--json` and listing so rows do not reshuffle (A/heap.go:8-14; A/live_scan.go:542-554). There is **no user-selectable sort key**; only live-sort mode (continuous vs freeze-on-move). Freeze-on-move: rows re-sort until the cursor moves, then freeze; first row pinned when finishing if cursor never moved (A/update.go:178-182, 259-260, 282-284, 310-329).
- **Live row accounting.** Child sizes apply immediately through a path index (`liveEntryIndex`); sorting and filter rebuild happen once per 100ms tick (`flushLiveEntryUpdates`); index invalidated on sort/remove/cancel/complete (A/update.go:184-257, 607-608).

### 2.3 Cache (disk + memory)

- **Location:** `~/.cache/mole/analyzer/<xxhash64(path) hex>.cache`, gob-encoded `cacheEntry`; overview snapshot store `~/.cache/mole/analyzer/overview_sizes.json` (A/cache.go:306-372, 153-159; A/constants.go:20,35).
- **Schema version** 13 (bumped for hardlink/clone/mount semantics changes, with a changelog in comments); mismatched or undecodable entries are deleted on read (A/cache.go:25-40, 594-621).
- **TTLs:** authoritative 7d (`analyzerCacheTTL`, `overviewCacheTTL`); stale-first-paint 3d (`staleCacheTTL`) (A/constants.go:18-19,25; A/cache.go:640-643, 677-679).
- **Freshness:** entry refused if directory mtime is after the recorded `ModTime` (any add/remove/rename), however recent; the TUI still paints stale data first and rescans behind it (A/cache.go:646-655, 660-682).
- **Admission:** a subtree earns a cache file only if `TotalFiles >= 100` or `TotalSize >= 10 MiB` (the median entry on a dev machine described a one-file directory; unbounded growth reached 1.88M files / 7.82 GB) (A/cache.go:379-381; A/constants.go:27-43). Listings are truncated to 30 entries (`maxEntries`) on disk with `NeedsRefresh=true` so browsing re-scans to show every child (A/cache.go:701-708; A/constants.go:9).
- **Eviction:** at startup (goroutine) delete expired, stale `.tmp` (>1h), then enforce 5000 entries / 50 MiB oldest-first with a bounded min-heap while streaming `ReadDir(512)` batches (A/cache.go:422-529; A/constants.go:36-42). Legacy flat `~/.cache/mole/*.cache` + `overview_sizes.json` are swept by 4 workers (A/cache.go:543-592).
- **Overview store:** single JSON `{path: {size, partial, updated, schema_version}}`, atomic temp+rename, 1000 entries cap with eviction to 900, write skipped when unchanged until within TTL/8 of expiry, `.corrupt` backup on parse failure (A/cache.go:42-47, 196-287; A/constants.go:49-51).
- **Atomic writes:** temp file in same dir + `publication.commit(rename)` so cancel and write are linearized (A/cache.go:721-750).
- **Invalidation:** after a delete, drop the cache entry and overview snapshot of each removed path and *every ancestor* (ancestor mtimes do not change on deep deletes) (A/cache.go:781-798); refresh `R` invalidates path + direct child dirs (A/cache.go:803-818).
- **In-memory:** `m.cache[path]` history of results with cursor/offset; `overviewSizeCache`; navigation back restores selection and scroll (A/model.go:134-147; A/update.go:1175-1251).
- **Background prefetch:** when launched on a specific path, a 30s-context prefetcher measures uncached overview rows with 8 workers (A/main.go:114-118; A/cache.go:849-883).

### 2.4 Screens / modes (state machine)

1. **Overview** (`isOverview && path=="/"`). Rows pending (`Size<0`) show spinner; sizes stream in; auto-sorted once all done (A/update.go:36-41).
2. **Directory view** (drill-down, live scan then browse). Substates: scanning, "Showing cached results while refreshing...", browsing, filtering (typing), delete-confirm.
3. **Top files view** (`showLargeFiles`): flat top-20 list of the *whole subtree*, own cursor, own filter, own multi-select.
4. **Deleting** (busy): hides rows; only `q`/`Q`/`Ctrl+C` quit and `r`/`R` are honored (A/update.go:636-648, view.go:67-91).
5. **Delete confirm** overlay line(s) under the footer (A/view.go:415-460).

### 2.5 Key bindings (complete, from `updateKey`)

| Context | Keys | Action | Source |
|---|---|---|---|
| any | `q` `Q` `Ctrl+C` | quit (in confirm: `q`/Esc cancel, Ctrl+C quits) | A/update.go:718-719, 697-703 |
| nav | `↑` `k` `K` / `↓` `j` `J` | move selection; overview skips hidden empty rows | A/update.go:738-760; A/model.go:359-384 |
| nav | `Enter` `→` `l` `L` | enter selected directory (file: status line "File: name, size"); no-op in Top files | A/update.go:761-765, 1284-1367 |
| nav | `Esc` | clear active filter, else leave Top files, else go back, else (overview) quit | A/update.go:720-737 |
| nav | `b` `B` `←` `h` `H` | leave Top files (clears filter) else go back | A/update.go:766-772 |
| action | `r` `R` | refresh (clears selections) | A/update.go:773-819 |
| action | `t` `T` | toggle Top files; "Top files are available after the scan finishes" while scanning | A/update.go:820-837 |
| action | `/` | start filter (needs non-empty list; not overview) | A/update.go:838-850 |
| filter typing | printable runes, `Space`, `Backspace`/`Delete` | edit query live; `Enter` apply; `Esc` clear; `Ctrl+C` quit; all other keys swallowed | A/update.go:1087-1173 |
| action | `s` `S` | toggle live sort mode (scanning, non-overview) | A/update.go:851-859 |
| action | `o` `O` | open (max 20 in batch: "Too many items to open, max 20") | A/update.go:860-905 |
| action | `f` `F` | reveal in Finder (max 20) | A/update.go:906-951 |
| action | `p` `P` | Quick Look/open file (single) | A/update.go:952-970 |
| action | `Space` | toggle multi-select; "Selection is available after the scan finishes" | A/update.go:971-1025 |
| action | `Delete` `Backspace` | open Move-to-Trash confirm | A/update.go:1026-1082 |
| confirm | `Enter` | execute | A/update.go:652-696 |
| confirm | `Esc` `q` | cancel ("Cancelled") | A/update.go:697-701 |
| window | resize | stores width/height | A/update.go:360-363 |

Deleted-batch ordering: deepest paths first (slash count) to avoid parent/child conflicts (A/delete.go:62-66).

### 2.6 Layout (concrete)

Output is a plain string, ANSI SGR via hand-written escapes, no lipgloss (A/view.go, A/constants.go:301-312). Colors: purple bold title (`1;35`), gray `0;38;5;244`, red `0;31`, yellow `0;33`, green `0;32`, blue `0;34`, cyan `0;36`. Spinner frames `⠖ ⠲ ⠴ ⠦` (shared with the shell UI in `lib/core/ui.sh`) (A/constants.go:298-299). Row cell widths are measured with `ansi.StringWidth`, truncation in the middle via `ansi.Truncate/TruncateLeft` (head = (w-3)/3, tail = rest; widths <10 hard-truncate) (A/format.go:38-59, 146-148).

Top to bottom, blank line first (A/view.go:15-17):

1. **Header.**
   - Overview: `Analyze Disk` (purple bold) + gray ` (X free)` ; optional gray snapshot line; then `Select a location to explore:` gray + spinner status (`Scanning N directories..., K left`, `Checking system folders...`, `Ready`) or `⠖ Analyzing disk usage...` while all rows pending (A/view.go:26-58, 464-475).
   - Directory: `Analyze Disk  <~/path>  |  Total: <size>[+]` (A/view.go:59-65).
2. **Scan status line** (while scanning): `<spinner> Scanning[ NN%]: <N> files, <N> dirs, <size>` with yellow counts, green size, cyan spinner; current path appended gray, middle-truncated, inline when `width - lineWidth - 2 >= 24` else on its own row. Fresh scans with no rows return only this block; cached or live scans continue to the list (A/view.go:93-153; A/constants.go:14). Numbers: `<1000` plain, then `1.2k`, `1.2M` (A/format.go:61-72).
3. **List.**
   - *Directory row* (A/view.go:361-368): `[prefix 3 cols: " ▶ " cyan bold or spaces][○|● select][ %*d. index][24-col bar][ percent 6 cols]  <|spinner>  <📁|📄> <name padded>  <size 10 cols right>  [hint]`. Name width = `clamp(termWidth-61, 24, 60)` minus extra index digits (A/format.go:150-162; A/view.go:297-298). Selected row paints number, percent, name and size cyan; multi-selected name green with green `●`.
   - *Overview row* (A/view.go:284-291): no select/icon; fixed 22-col name; `percent` of the sum of known sizes; hint = unused time. Pending rows render `⠖ scanning` right-aligned; unknown percent renders `  --  `. Empty finished rows hidden except mount points (A/model.go:84-88).
   - *Bar*: 24 cells, **sub-cell resolution in eighths** using `█▏▎▍▌▋▊▉`; length = `round(value/max*24*8)/8` computed in float64 (int overflow at 42.7 PB); below 1/8 cell draws a gray `▏` tick; color by percent of total: >=50 red, >=20 yellow, >=5 blue, else green; bar length is relative to the max row, color is relative to the total (A/format.go:19-21, 91-144).
   - *Size column*: `humanizeBytes` (SI), 10 cols, partial adds `+` in last column, unavailable prints `unknown`; size color by percent (red/yellow/blue/gray) (A/format.go:218-238; A/view.go:549-560).
   - *Percent*: `%6s` of `x.y%`, `< 0.1%` for tiny, `  --  ` when unknown/partial (A/format.go:78-89).
   - *Top-files row* (A/view.go:204-205): `[prefix][○|●] NN. [bar vs max file]  |  📄 <~/full/path middle-truncated>  <size>` (bar color arg percent 0 so green).
   - Viewport rows: `termHeight - 6` (dir) / `-5` (top files), clamped 1..30; default 12 when height unknown (A/view.go:573-593; A/constants.go:17).
   - Empty states: `Empty directory`, `No large files found`, `No matches for "q"`. Filter line above list: `Filter: <q>▌  (N matches)` (A/view.go:155-224).
4. **Blank line, then footer hint line (gray), by context** (A/view.go:374-414):
   - overview, with history: `↑↓←→ | Enter | R Refresh | O Open | P Preview | F File | Esc Back | Q/Ctrl+C Quit`; without: `↑↓→ | Enter | R Refresh | O Open | P Preview | F File | Esc/Q Quit`.
   - directory: `↑↓←→ | Space Select | Enter | / Filter | R Refresh | O Open | P Preview | F File | ⌫ Del[ N] | T Top N | Esc Back | Q/Ctrl+C Quit` (`T Top N` shows only when large files exist; `Del N` shows selected count).
   - Top files: `↑↓← | Space Select | / Filter | R Refresh | O Open | P Preview | F File | ⌫ Del[ N] | Esc Back | Q/Ctrl+C Quit`.
   - filter typing: `Type to filter  |  Enter Apply  |  Esc Clear  |  Ctrl+C Quit`; filter applied: `... / Edit | Esc Clear filter ...`.
5. **Delete confirm block** (red `Move to Trash:` + `name, size` or `N items, size` + gray `Press Enter to confirm  |  ESC cancel`), plus yellow app-bundle hint lines (A/view.go:415-460).
6. **Deleting view:** header then `⠖ Moving to Trash, please wait...` (or `... N items moved, please wait...` once the counter > 0; counter is per selected path since a recursive count made big deletes look hung) (A/view.go:67-91; A/delete.go:120-122).

Status-line strings are tracked in `m.status` but only the overview shows it in the header; directory view shows scan summary only via its header (A/view.go:464-475). There is no persistent bottom status bar in directory view.

### 2.7 Overview rows (what "insights" and "cleanable" mean)

Rows in order: Home (`$HOME`, excluding ~/Library), User Library (`~/Library`; renamed from "App Library"), Applications (`/Applications`), System Library (`/Library`), mounted `/Volumes/*` disks, then insights (A/main.go:187-215, 217-222).

Insight rows (only if the path exists; hidden when measured size is 0) (A/insights.go:15-92):

| Name | Path |
|---|---|
| iOS Backups | `~/Library/Application Support/MobileSync/Backup` |
| Old Downloads (90d+) | `~/Downloads` (special measure: only **non-hidden top-level entries with mtime older than 90 days**; dirs sized with `du`, files by `st_size`; skip other devices) (A/insights.go:103-159) |
| System Logs | `~/Library/Logs` |
| Homebrew Cache | `~/Library/Caches/Homebrew` |
| Xcode DerivedData / Simulators / Archives | `~/Library/Developer/Xcode/DerivedData`, `~/Library/Developer/CoreSimulator/Devices`, `~/Library/Developer/Xcode/Archives` |
| Spotify Cache | `~/Library/Application Support/Spotify/PersistentCache` |
| JetBrains Cache | `~/Library/Caches/JetBrains` |
| Docker Data | `~/Library/Containers/com.docker.docker/Data` |
| pip / uv / Gradle / CocoaPods Cache | `~/Library/Caches/pip`, `~/.cache/uv`, `~/.gradle/caches`, `~/Library/Caches/CocoaPods` |
| OrbStack Data | glob `~/Library/Group Containers/*dev.orbstack/data` (first match) |

`~/Library/Caches` is deliberately **omitted** because the listed cache rows are its children and would double count (A/insights.go:45-48). JSON overview flags these `"insight": true` (A/json.go:181).

"Cleanable" (🧹) = directory row where (a) path contains none of `/Library/Caches/`, `/Library/Logs/`, `/Library/Saved Application State/`, `/.Trash/`, `/Library/DiagnosticReports/` (those are `mo clean` territory), and (b) either it holds a valid `CACHEDIR.TAG` (first 43 bytes equal `Signature: 8a477f597d28d172789f06886806bc55`, regular file via Lstat) or its basename is in `projectDependencyDirs` (node_modules, venv, __pycache__, target, build, dist, DerivedData, Pods, .gradle, .terraform, ... ~45 names) (A/cleanable.go:12-144). JSON: `"cleanable": true` for dirs (A/json.go:177).

### 2.8 Delete-from-TUI flow

1. Backspace/Delete (scan finished; not overview) opens confirm; unavailable-size rows refused ("Cannot move X to Trash, size unknown") (A/update.go:1026-1082).
2. Enter: collect **paths** (multi-select map or the target), build `deleteAudit` with sizes the view showed, run `deletePathCmd` (single) or `deleteMultiplePathsCmd` (deepest first) (A/update.go:651-696; A/delete.go:38-98).
3. Each path: `Lstat` (broken symlinks OK) then `moveToTrash`: `validateTrashTarget` on raw input *before* `Abs` resolves `..`, then on the absolute path and its `EvalSymlinks` result (A/delete.go:113-162, 309-320). Methods in order: (1) `/usr/bin/trash <abs>` (macOS 15+; absolute path, no `--`; works over SSH without Finder dialogs), 30s timeout; (2) headless atomic `renameatx_np(..., RENAME_EXCL)` into `~/.Trash` on the home volume or `<mount>/.Trashes/<uid>` otherwise, up to 100 collision-renamed attempts `name.<ns>.<pid>.<n>`, trash dir must be a non-symlink dir owned by the user and not group/world writable; (3) Finder AppleScript `tell application "Finder" to delete POSIX file ...`, 30s (A/delete.go:23-36, 164-307).
4. **Never permanent deletion.** There is no `rm` path.
5. Protection (`isProtectedAnalyzeDeletePath`) refuses: EDR/MDM Darwin cache subtrees (`/private/var/folders/...` structural depth <=3 or containing bundle prefixes `com.crowdstrike.`, `com.sentinelone.`, `com.sentinel-labs.`, `com.eset.`, `com.jamf.`, `com.jamfsoftware.`, `com.paloaltonetworks.`, `com.cisco.anyconnect`, `com.cisco.secureclient`, `com.alibaba.security.`); critical roots (`/`, `/Applications`, `/Library`, `/System`, `/Users`, `/Volumes`, `/usr`, `/opt`, `/opt/homebrew`, `/private/*` etc.), every direct child of `/Users`, system trees (`/System`, `/bin`, `/sbin`, `/usr`, `/private/etc`, `/Library/Apple`, `/Library/Extensions`, `/Library/Keychains`, Finder.app, Safari.app, `/dev`); direct children of `/Library`; the home root; home head folders `Library, Documents, Desktop, Downloads, Pictures, Movies, Music` (case-insensitive and by file identity; their *contents* stay deletable); direct children of `~/Library`; Docker Desktop state (`~/Library/Containers/com.docker.docker`) and OrbStack (`~/.orbstack`, `Group Containers/*dev.orbstack*`) including **ancestors** that contain an existing store; iCloud Drive structural paths (Mobile Documents, each app container, each container's `Documents` dir; files at those depths stay user content) (A/delete.go:322-632, 638-673). Home roots protected include `$HOME`, `user.Current().HomeDir` and their symlink-resolved forms (A/delete.go:566-589). Same-file comparison uses `os.SameFile` to defeat symlink and case aliases.
6. Audit: operations log `~/Library/Logs/mole/operations.log` lines `[YYYY-MM-DD HH:MM:SS] [analyze run=<ts-pid-rand-rand>] TRASHED <path> (<size>)` or `FAILED <path> (<fixed reason>)`, framed by `# ========== analyze run=... session started/ended at ..., N items, SIZE ==========`; plus `deletions.log` (`$MOLE_DELETE_LOG` or `~/Library/Logs/mole/deletions.log`) TSV rows `ISO8601<TAB>trash<TAB>sizeKB<TAB>ok|trash-failed<TAB>path`. Control bytes escaped as `\n \r \t \xHH`; failure reasons are fixed strings (`protected`, `removal timed out`, `permission denied`, `trash-failed`); log failure appends " · audit log unavailable" to the status; `MO_NO_OPLOG=1` disables operations.log only (A/oplog.go:16-226).
7. After completion: remove rows from view, subtract size, cancel live/overview/background work touching removed paths **before** invalidating caches, invalidate ancestry, mark touching history/cache `NeedsRefresh`, reset overview rows to pending, restart a scan of the current path (A/update.go:364-425). Status: `Moved N items to Trash` (or `, some failed: <first 3 errors joined>`) (A/update.go:385-388; A/delete.go:105-110).
8. App bundles: confirm shows yellow note "Only the app bundle moves to Trash. Use mo uninstall <App> to also remove support files." (A/view.go:453-459).

### 2.9 Size formatting semantics

- **Analyze uses SI (1000-based)** to match Finder/diskutil: `<1000` → `N B`; otherwise one decimal and `kMGTPE` suffix (`1.5 MB`); a value that rounds to `1000.0` steps up a unit; negative clamps to `0 B` (U/bytes.go:16-43). Status uses binary (1024) helpers in the same package (U/bytes.go:45-95).
- Sizes on disk are **allocated** bytes (blocks), not apparent size (A/scanner.go:1328-1339); `du` totals are KiB-granular (A/scanner.go:1095).
- Partial: `12.3 GB+`; unavailable or partial-zero: `unknown`; header `Total:` uses the same label; scan summary `Scanned <size>` or `Partial scan · <size>` (A/format.go:218-245).
- Operations-log size text mirrors shell `bytes_to_human`: `1.23GB`, `4.5MB`, `12KB`, `7B` (A/oplog.go:96-111).

### 2.10 JSON output (`--json`)

Top level (A/json.go:15-23): `scan_status` ("complete"|"partial"|"unavailable"), `path`, `overview` (bool), `entries[]`, `large_files[]` (omitted if empty; objects `{name,path,size}`), `total_size`, `total_files` (omitted if 0).
Entry (A/json.go:25-34): `scan_status`, `name`, `path`, `size` (bytes), `is_dir`, `insight` (overview only, omitempty), `cleanable` (dirs only, omitempty), `last_access` (RFC3339 UTC, omitempty).
- Directory JSON uses the full blocking scan: **all** direct children (not capped at 30), large files via walk + Spotlight, `scanPathConcurrentAllEntries`; scan error → stderr + exit 1 (A/json.go:60-80).
- Overview JSON measures all rows with 8 workers; uses the overview cache if present and >0, else fresh `du` with 30s timeout (`duTimeout`); omits empty complete rows; sorted by size desc; `total_size` is the sum (A/json.go:82-166).
- `--json` runs also launch `pruneAnalyzerCache` in a goroutine (A/main.go:77).
- `ScanStatus` JSON text via `MarshalText` (A/model.go:41-46).

---

## 5A. Go dependencies used by analyze (go.mod, shared with status)

| Go dep (version) | Use | Suggested Rust crate (suggestion only) |
|---|---|---|
| `charmbracelet/bubbletea` v1.3.10 (repo:go.mod:7) | Elm-style TUI loop, alt screen, key messages, `tea.Tick` (A/main.go:120; A/update.go:143-147) | `ratatui` + `crossterm` with a hand-written message/reducer loop, or `tokio` + channels for the scan event stream |
| `charmbracelet/lipgloss` v1.1.0 (repo:go.mod:8) | status only (see Part B); analyze uses raw ANSI | `ratatui` styles |
| `charmbracelet/x/ansi` v0.11.9 (repo:go.mod:9) | display width and ANSI-aware truncate (A/format.go:14, 146-148) | `unicode-width` + `unicode-segmentation` (grapheme-aware), `ratatui`'s own width handling |
| `cespare/xxhash/v2` v2.3.0 (repo:go.mod:6) | cache file name hash (A/cache.go:364) | `xxhash-rust` (xxh64) or `twox-hash` |
| `golang.org/x/sys` v0.50.0 (repo:go.mod:11) | `getfsstat`, `renameatx_np`, `statfs`, raw `getattrlist` (A/volumes.go:15-20; A/delete.go:207; A/clone_darwin.go:47) | `libc` or `nix` (`statfs`, `getfsstat` via libc), `rustix` |
| stdlib `encoding/gob` | cache entries (A/cache.go:607, 734) | `bincode`/`postcard` + `serde` (version tag as in schema 13) |
| stdlib `encoding/json`, `flag` | JSON out, flag parse | `serde_json`, `clap` |
| stdlib `os/exec` | `du`, `mdfind`, `open`, `trash`, `osascript`, `tmutil` | `std::process::Command` (with timeouts via `wait-timeout` or tokio) |
| stdlib goroutines + semaphores | scan concurrency | `rayon` (bounded pools) or `tokio` + `Semaphore`; `jwalk`/`ignore::WalkParallel` for the walk |
| Go `container/heap` | top-N | `std::collections::BinaryHeap` |

Not used by analyze: gopsutil.

## 6A. Porting notes (analyze)

Performance:
- Never spawn one OS thread per blocked syscall; macOS kills the process past the per-user thread cap. Keep walker pools at <=12, `du` at <=4, queue goroutines bounded (A/constants.go:53-62; A/scanner.go:143-154).
- Prefer one `du -skPx` per folded/overview subtree over a user-space walk: it is both faster and handles hardlink dedupe (A/scanner.go:1121-1122). Parse stderr for permission-only failures (A/scanner.go:1125-1141).
- Memoize the cache directory `MkdirAll` result; it was 470ms and 200MB of garbage per whole-disk scan (A/cache.go:334-338). Build cache path by concatenation, not path join (A/cache.go:362-363).
- Admission-control the on-disk cache (>=100 files or >=10 MiB) and cap count/bytes; stream directory reads in batches of 512 (A/cache.go:379-381, 439-446).
- Overview store: skip rewrite if size unchanged and entry fresh; no JSON indentation (A/cache.go:208-215, 253-255).
- Sort once per UI tick, not per event; keep a path index for O(1) child updates (A/update.go:236-257).
- Bound event channel and make progress lossy, reserve completion slots (A/live_scan.go:40-79).
- Bar math in float to avoid i64 overflow at 42.7 PB (A/format.go:109-116).
- Overview rows read the kernel mount table with `MNT_NOWAIT` (A/volumes.go:12-13).

Correctness:
- Classify measurement as complete/partial/unavailable everywhere; never render unavailable as 0; persist only complete or permission-only partial results (A/model.go:48-117; A/format.go:218-229).
- Directory mtime newer than cached mtime invalidates immediately (no grace window); ancestors must be invalidated after deletes (A/cache.go:646-655, 781-798).
- Cancel before invalidating, and linearize cancel with publish (`scanPublication`); reject results by (id, path) (A/update.go:1382-1384; A/scanner.go:77-130).
- Hardlink dedupe is order dependent: do not persist subtrees that used it (A/scanner.go:705-712).
- APFS clone dedupe only when the file `shares all blocks`; clone id is volume-local, key by (dev, id); put clone attrs in the fork attribute group (A/clone_darwin.go:12-20; A/scanner.go:1287-1326).
- Stop at mount points for every traversal, including `du -x` operands that are themselves mount points (A/scanner.go:1200-1203, 1228-1230; schema v12/v13 history A/cache.go:38-39).
- Use `st_blocks*512` capped by size; symlinks sized as links and never followed (A/scanner.go:1328-1339, 378-403).
- Do not touch iCloud FileProvider/CloudStorage in the overview walk (materialization can block tens of seconds) (A/constants.go:68-78).
- Selection must be keyed by path; changing a filter query clears selection (A/model.go:250-251; A/update.go:1113, 1156).
- Fit text by display cells, not bytes/runes (NFD names, CJK, joined emoji); cut long names in the middle so sibling tails stay distinct (repo:.claude/rules/go-tuis.md Status/Tests bullets; A/format.go:38-59).
- Trash only; validate paths before and after `Abs`, again after `EvalSymlinks`; compare protected paths by file identity (`SameFile`) since APFS is case-insensitive; refuse EDR cache subtrees (A/delete.go:138-162, 309-320, 322-406, 638-673).
- Keep spinner frames in sync with the shell UI (`lib/core/ui.sh`) (A/constants.go:298).
- Overview `Home` excludes `~/Library` and the `Library` row is measured separately so totals do not double count; `~/Library/Caches` is left out of insights for the same reason (A/scanner.go:950-955; A/insights.go:45-48).
- Directory view always lists **every direct child** after a live scan, while cached summaries are bounded to 30 entries and force a refresh before browsing (A/cache.go:701-708; repo:.claude/rules/go-tuis.md Analyze bullet 5).


---

# Part B: `mo status`

The STATUS feature table is in B.A; STATUS rows are the status counterpart of section 1.


All paths relative to ../mole. `cmd/status/` abbreviated `S/`.

### B.A. Feature table

| ID | Command | Flags/options | Behavior | Privilege | Safety rule | Source |
|---|---|---|---|---|---|---|
| STATUS-01 | `mo status` (TTY stdout) | none | Interactive Bubble Tea dashboard on the alt screen. | user; no sudo | read-only, no writes except prefs file | S/main.go:300-306,392-396; mole:338-339 |
| STATUS-02 | `mo status` (stdout not a TTY) | auto | Auto-switches to one pretty-printed JSON snapshot when stdout is not a char device (pipe/redirect). | user | read-only | S/main.go:36-48,392-393 |
| STATUS-03 | `mo status --json` | `--json` | Force one-shot JSON (2-space indent). Uses the full `Collect()`. Per-collector error goes to stderr, the partial snapshot still prints and exits 0. Exit 1 if no core metric (CPU logical, mem total, process time, or any disk total>0) or encode error. | user | read-only | S/main.go:26,252-297 |
| STATUS-04 | `mo status --watch` | `--watch`, `--interval DURATION` (Go duration, >0, default 1s) | NDJSON stream, one full snapshot per line from one warm collector (rates stay valid). First snapshot is immediate; exits silently when stdout closes. Invalid interval: exit 1. | user | read-only | S/main.go:32-33,308-321,381-390; S/watch.go:13-40 |
| STATUS-05 | process CPU alerts | `--proc-cpu-alerts` (bool, default true), `--proc-cpu-threshold PERCENT` (float, default 100, must be >=0), `--proc-cpu-window DURATION` (default 5m, must be >0) | A process is flagged "active" after staying at or above the threshold for the whole window. Shown as a banner in the TUI and in JSON `process_alerts` and `process_watch`. | user | only observes, never kills | S/main.go:27-29,99-107; S/process_watch.go:64-150 |
| STATUS-06 | help | `-h`, `--help` | Hand-written usage to stdout, exit 0. Bad flag or invalid value: message plus "Use 'mo status --help'" on stderr, exit 1 (not Go's exit 2). | none | n/a | S/main.go:323-374 |
| STATUS-07 | key `q` / `esc` / `ctrl+c` | n/a | Quit. | n/a | n/a | S/main.go:117-118 |
| STATUS-08 | key `k` | n/a | Toggle the animated mole ("cat"); persisted as `cat_hidden`. | writes `~/.config/mole/status_prefs` | atomic write under flock, silent on failure | S/main.go:119-123; S/prefs.go:144-151 |
| STATUS-09 | key `c` | n/a | Cycle the number of per-core rows 2 -> 4 -> 8 -> all(0); persisted as `cpu_cores`. | same | same | S/main.go:124-128; S/prefs.go:155,160-186 |
| STATUS-10 | launch | `bin/status.sh` execs `bin/status-go "$@"` | Shell wrapper. If the binary is missing it prints "Bundled status binary not found... run mo update" and exits 1. Also launched from the interactive menu, item 5 or key `5`. | user | n/a | bin/status.sh:7-15; mole:268,284,338 |
| STATUS-11 | build | `make build` | `go build -ldflags="-s -w" -o bin/status-go ./cmd/status`. Release targets are pure Go, CGO off. | n/a | n/a | Makefile:14-18,38-42 (release: Makefile:20-21,"RELEASE_GO_ENV") |

Flags found for status: `--json`, `--watch`, `--interval`, `--proc-cpu-alerts`, `--proc-cpu-threshold`, `--proc-cpu-window`, `-h/--help`. No path arguments, no `--version`. Hidden env: `MOLE_TEST_MODE=1` / `MOLE_TEST_NO_AUTH=1` disable the Finder osascript call (S/metrics_disk.go:460-474).

### B.B. Metrics catalog and collection (macOS)

Everything is collected by gopsutil plus shelling out to system binaries. There is no IOKit, SMC or direct sysctl syscall code in Go. The two `sysctl` uses are subprocesses. `runCmd` forces `LC_ALL=C` and strips other LC_*/LANG vars (locale decimal commas broke parsing, #1267) (S/metrics.go:717-742).

#### Cadence (scheduler)
- `refreshInterval` 1s; `processWatchInterval` 1s; `slowRefreshInterval` 30s (S/main.go:18-22).
- `nextMode`: no snapshot yet -> Fast; no full in 30s -> Full; no process sample in 1s -> Process; else Fast (S/collection.go:43-54). So the cadence is one Full every 30s, a Process collection (fast plus ps) every tick otherwise. Pure Fast only happens first.
- After the first Fast snapshot the next tick is immediate (0 delay) so the first Full lands right away. Later the delay is 1s after completion, not a fixed rate (S/collection.go:58-70). Attempts consume the interval even on failure.
- TUI collects off the Update loop via `tea.Cmd`, and `collecting` flag skips overlapping ticks (S/main.go:134-139).
- All tasks in a collection run concurrently with panic recovery, and errors are merged (S/metrics.go:339-374).
- Full collection: CPU is sampled FIRST and alone, before subprocess burst, so the 100ms window is not polluted by Mole's own children (#1237) (S/metrics.go:427-436).
- Fast/Process snapshots reuse the last Full "enrichment" (hardware, P/E counts, mem cache and pressure, corrected disks, GPU, trash, proxy, batteries, thermal, bluetooth) and recompute the health score from the merged data (S/metrics.go:658-701). Process data from the last collection is re-applied as stale (ProcessStale=true) when a snapshot has no live process data (S/metrics.go:703-715).

#### Per metric
| Metric | Source on macOS | Interval / cache / timeout | Where |
|---|---|---|---|
| Host, platform, uptime, procs | gopsutil `host.Info()` | every collection | S/metrics.go:331-337,587-594 |
| CPU total and per-core | gopsutil `cpu.Times(true)` twice, 100ms apart (`cpuSampleInterval`), after a `cpu.Percent(0,true)` warm-up. Per-core denominator floored at wall-clock elapsed so a parked Apple Silicon core is not reported at ~100%. Total = sum(busy)/sum(window) over cores. Busy = user+system+nice+irq+softirq+steal. | Full path only. Fast path uses `cpu.Percent(0,true)` (delta since last call). If it fails: Fast zeroes per-core and sets `per_core_estimated`; Full falls back to `ps -Aceo pcpu` (500ms) summed/logical, spread evenly. | S/metrics_cpu.go:16-17,28-96,294-368 |
| Load avg 1/5/15 | gopsutil `load.Avg()`; Full-only fallback to parse `uptime` (500ms) when error or all zero | per collection | S/metrics_cpu.go:87-96,188-238 |
| Core counts | gopsutil `cpu.Counts(false/true)`, `runtime.NumCPU` fallback | per collection | S/metrics_cpu.go:29-40 |
| P/E core split | subprocess `/usr/sbin/sysctl -n hw.perflevel0.logicalcpu hw.perflevel1.logicalcpu`, 500ms, cached 10 min. Order decides the bucket, not the name (M5 names levels "Super"/"Performance"); both must be >0. | Full only | S/metrics_cpu.go:122-186 |
| Memory | gopsutil `mem.VirtualMemory` (used, total, available, used%), `mem.SwapMemory` | every collection | S/metrics_memory.go:21-52 |
| Memory "Cached" | `vm_stat` (500ms) parse "File-backed pages" times page size from header; only when gopsutil Cached==0 | Full only | S/metrics_memory.go:36-40,54-91 |
| Memory pressure | subprocess `/usr/sbin/sysctl -n kern.memorystatus_vm_pressure_level`: 1 normal, 2 warn, 4 critical, else "" (500ms) | Full only | S/metrics_memory.go:93-123 |
| Disks | gopsutil `disk.Partitions(false)` + `disk.Usage`. Skip: /dev/loop*, `/System/Volumes/*`, `/private/*`, `/dev`, fstypes afpfs/autofs/cifs/devfs/fuse*/macfuse/nfs/osxfuse/procfs/smbfs/tmpfs/webdav, and on macOS any device not starting `/dev/`. Dedupe by APFS base disk (`diskNsM` -> `diskN`). Skip volumes <1GiB. Sort internal first then size desc. Cap 3. | Fast = raw statfs. Full adds corrections below. | S/metrics_disk.go:20-45,59-177 |
| Disk total fix | `diskutil info -plist <mount>` (3s timeout, memoized once per mount per pass); keys TotalSize/DiskSize/Size; replaces statfs total if they differ by >1GiB (fixes doubled external APFS) | Full only | S/metrics_disk.go:352-410 |
| Disk used (APFS) | 3 tiers: (1) Finder via `osascript -e 'tell application "Finder" to return {free space of startup disk, capacity of startup disk}'` for "/" only, 5s timeout, cached 2 min, failure also cached 2 min; purgeable = finderFree - statfsFree; (2) `APFSContainerFree` from the plist, used only if it differs from raw used by >1GiB; (3) raw statfs. | Full only | S/metrics_disk.go:412-512 |
| Disk external flag and SMART | plist keys `Internal` and `SMARTStatus` (else `diskutil info <disk>` text, 1s). SMART normalized to verified/failing/unsupported/unknown. External fallback = mount under /Volumes/. Metadata cached 2 min per base device (global map, no lock). Fast path sets External by `/Volumes/` prefix only. | Full only | S/metrics_disk.go:179-345 |
| Disk I/O | gopsutil `disk.IOCounters()` summed read/write bytes; MB/s = delta/1MiB/elapsed. First sample returns 0. Counter wrap returns 0. | every collection | S/metrics_disk.go:582-626 |
| Trash size | `filepath.WalkDir(~/.Trash)`, symlinks skipped, 2s timeout then `trash_approx=true`; cached 5s. Collected and in JSON but NOT rendered (renderDiskCard ignores its args). | Full only | S/metrics_disk.go:196-203,628-678; S/view.go:496 |
| Network rates | gopsutil `net.IOCounters(true)` (panic-guarded) every collection; MB/s = delta/1MiB/elapsed (elapsed floored 100ms). Counters primed in `NewCollector`. | 1s | S/metrics_network.go:35-140 |
| Network filter | hide prefixes lo, awdl, utun, llw, bridge, gif, stf, xhc, anpi, ap (and tun/ipsec/ppp tunnels) unless it is the default-route interface; sort tunnel-default first then rx+tx desc; cap 3 but force the default interface into slot 3. | | S/metrics_network.go:24,160-167,89-131 |
| Default route | subprocess `route -n get default`, 500ms, parse `interface:`; Full only, cached on the collector | 30s | S/metrics_network.go:46-50,144-158 |
| Totals and history | totals = routed tunnel if default is a tunnel, else sum of rows; ring buffers of 120 samples each for rx/tx | 1s | S/metrics_network.go:171-180; S/metrics.go:190 |
| Interface IPs | gopsutil `net.Interfaces()`, first IPv4 non-127; cached 10s | | S/metrics_network.go:182-208 |
| Proxy | env (https/http/all_proxy, either case) -> `scutil --proxy` (500ms; SOCKS, HTTPS, HTTP, PAC, WPAD in that order) -> active `utun*`/`tun*` with nonzero bytes => type "TUN", `IsTunnel`, shown as "Tunnel" | Full only | S/metrics_network.go:220-349 |
| Battery level, status, time left | `pmset -g batt` (1s); parse percent, token after %, "remaining" | Full only | S/metrics_battery.go:28-127 |
| Battery cycles, condition, max capacity | `system_profiler SPPowerDataType -json` (3s, cached 30s) with text fallback; then `ioreg -rn AppleSmartBattery` (500ms): CycleCount (0<x<100000) overrides; capacity from DesignCapacity vs AppleRawMaxCapacity (else NominalChargeCapacity) used only when system_profiler gave none | Full only | S/metrics_battery.go:129-344 |
| Battery temp, power | `ioreg -rn AppleSmartBattery` (500ms): Temperature (centi-degC, or degC if <1000), AdapterDetails Watts, SystemPowerIn/SystemPower (mW->W, 0..1000W), BatteryPower or voltage*amperage derived (+ = discharge, +-200W range); macOS 27 fallback `ioreg -rc AppleSmartBatteryPack -l` | Full only | S/metrics_battery.go:346-550 |
| Fans | `fan`+`speed` line in cached `system_profiler SPPowerDataType` text, first int. `FanCount` is never set. | Full only | S/metrics_battery.go:353-366 |
| CPU/GPU temperature | NOT collected on macOS. `collectThermal` never sets `CPUTemp`/`GPUTemp`; the comment says battery sensors and cpu_thermal_level are deliberately not used as CPU temp. So `@ temp` in the CPU card, the CPU temp health penalty and "CPU temperature high" can't fire on Mac in this build. | | S/metrics_battery.go:390-392; grep: only parse sets BatteryTemp |
| Sensors | struct and JSON `sensors` exist, collection disabled | | S/metrics.go:479-480,725 (view.go:725) |
| GPU info | `system_profiler -json SPDisplaysDataType` (4s, cached 10 min): name, VRAM, metal, vendor, `sppci_cores` -> core count. Usage -1 until read. | Full only | S/metrics_gpu.go:14-19,94-153 |
| GPU usage | `powermetrics --samplers gpu_power -i 500 -n 1` (2s timeout, cached 5s, regex "GPU HW active residency", else 100-idle; -1 on error). Needs root, so usually -1 without sudo. Non-darwin: `nvidia-smi` CSV. Applied to GPU[0]. GPU is in JSON; the TUI shows only the GPU core count in the header ("M1 Pro, 16GPU"), no GPU card. | | S/metrics_gpu.go:155-196; S/view.go:182-184,717-724 |
| Hardware identity | `system_profiler SPHardwareDataType` (3s: Model Name, Chip / Processor Name), `sw_vers -productVersion` (1s), `system_profiler -detailLevel mini SPDisplaysDataType` (2s) parse max "NN Hz" (<500); the three run in parallel; refreshed every 10 min; RAM and Disk size from memory and disk[0] | | S/metrics_hardware.go:15-106; S/metrics.go:502-518 |
| Bluetooth | `system_profiler SPBluetoothDataType` (4s) parsed by Connected:/Not Connected: group; fallback `bluetoothctl info`; cached 30s. In JSON only, not rendered. | Full only | S/metrics_bluetooth.go:11-153; S/metrics.go:488-499 |
| Processes | `ps -Aceo pid=,ppid=,state=,pcpu=,pmem=,rss=,comm= -r` (3s, strict parse); fallback `ps aux` (strict header check, no ppid). Top 5 kept by a bounded min-heap (CPU desc, mem desc, pid asc); card shows 3. Name = basename of comm. Zombie count (`Z*` state) plus top 3 parent PIDs with counts, `zombie_parents_complete` flag. Zombies are JSON-only. | every Process or Full collection (1s) | S/metrics_process.go:15-47,221-261,143-200; S/metrics.go:561-573 |
| Process alerts | tracked by (pid, ppid, command); `firstAbove` set when cpu>=threshold; triggered when held >= window; dropped when it falls below or disappears; status always "active" | each live process sample | S/process_watch.go:64-111 |

JSON schema (top level): collected_at, host, platform, uptime, uptime_seconds, procs, hardware{model,cpu_model,total_ram,disk_size,os_version,refresh_rate}, health_score, health_score_msg, cpu{usage,per_core,per_core_estimated,load1/5/15,core_count,logical_cpu,p_core_count,e_core_count}, gpu[], memory{used,total,available,used_percent,swap_*,cached,pressure}, disks[], trash_size, trash_approx, disk_io{read_rate,write_rate MB/s}, network[], network_history{rx,tx}, proxy, batteries[], thermal{cpu_temp,gpu_temp,battery_temp,fan_speed,fan_count,system_power,adapter_power,battery_power}, sensors, bluetooth, top_processes, process_collected_at, process_stale, zombie_*, process_watch, process_alerts (S/metrics.go:62-95, structs 97-234). A test (`TestMetricsSnapshotFieldsHaveCollectionClassifications`) forces every new field to be classified fast/full (S/metrics.go:290).

### B.C. Health score and diagnosis

Score starts at 100, subtract penalties, clamp 0..100, `int()` truncation (S/metrics_health.go:64-214). Constants at S/metrics_health.go:9-62.

| Component | Weight | Formula |
|---|---|---|
| CPU usage u | 30 | u<=50: 0. 50<u<=85: 15*(u-50)/35. u>85: 30*(u-50)/50. Issue "High CPU" at u>85. |
| Memory used% m | 25 | m<=70: 0. 70<m<=88: 12.5*(m-70)/18. m>88: 25*(m-70)/30. "High Memory" at m>88. |
| Memory pressure | | warn -5 ("Memory Pressure"), critical -15 ("Critical Memory"). |
| Disk (disks[0] used%) d | 20 | d<=80: 0. 80<d<=93: 10*(d-80)/13. d>93: 20*(d-80)/20. "Disk Almost Full" if `diskIsCritical` (>=90% used AND free <30,000,000,000 bytes). |
| SMART | | any disk failing: cap score at 44, "Disk SMART Failing". |
| Thermal (only if CPUTemp>0) | 15 | T<=65: 0. 65<T<=85: 15*(T-65)/20. T>85: 15 plus "Overheating". Dead on macOS (see B). |
| Disk I/O (read+write MB/s) | 10 | <=50: 0. 50-150: 10*(x-50)/100. >150: 10 plus "Heavy Disk IO". |
| Battery (batteries[0]) | | `batteryHealthLabel`: capacity>0 && <60 danger; cycles>1000 or capacity<80 warn; capacity<=0 unknown; else ok. danger -5 ("Battery Service Soon"), warn -2. |
| Uptime | | >14d -3 ("Restart Recommended"); >7d -1. |

Band message: >=85 "Excellent", >=65 "Good", >=45 "Fair", else "Needs Attention"; `msg + ": " + issues joined by ", "` (S/metrics_health.go:196-212). Score color bands use the same thresholds (S/view.go:290-301).

Diagnosis line (shown after "Health N" only when no error) picks the first match in order (S/diagnosis.go:8-55):
1. any disk SMART failing -> "SMART failing, back up now"
2. CPU > 85 -> "<top proc, if top CPU >=50 and process sample fresh> high CPU" else "CPU load high"
3. memory pressure warn/critical or used% > 88 -> "<top-mem proc> memory pressure" or "Memory pressure high"
4. root disk critical -> "Disk low, <N> free"
5. battery capacity<80 -> "Battery health low"; cycles>1000 -> "Battery cycles high"
6. CPU temp>65 -> "CPU temperature high" (unreachable on Mac)
7. disk I/O >150 MB/s -> "Disk I/O busy"
8. if health msg contains ":" -> the full health msg
9. "All clear"

### B.D. UI, keys, layout

Screens/modes: one screen only. "Loading..." until the first snapshot (S/main.go:163-166). No help overlay, no footer or key legend, no scrolling, no sort/select. Keys: `q`, `esc`, `ctrl+c` quit; `k` toggle mole; `c` cycle cores (S/main.go:115-129). `tea.WithAltScreen`, no mouse (S/main.go:301). Resize via WindowSizeMsg (S/main.go:130-133).

Top-to-bottom layout (S/main.go:163-230):
1. Header line: `Status` (bold #C79FD7) + 2 spaces + `Health ● <score>` (score colored: >=85 #87FF87, >=65 #87D787, >=45 #FFD75F, else #FF6B6B, bold) + diagnosis in grey #737373, then `  ` + identity parts joined ` · `: model (#BD93F9), `<chip>[, NGPU]`, `RAM x`, `Disk y`, refresh Hz, OS version, `up 3d 4h` (warn yellow >7d, red with ↻ >14d). Width fitting tries candidate sets in order (all, minus optional, identity+specs, first identity+specs, specs only) then drops tail parts. Compact (<=80 cols) omits OS version and uptime (S/view.go:159-288).
2. Error row `ERROR: ...` in red when the last collection failed (S/view.go:278-283).
3. Alert bar: yellow background #FFD75F, text #2B1200, bold, padding 1; "ALERT name (pid) at 123.4% for 5m0s (threshold 100.0%) · +N more"; prefix "LAST ALERT" or "OLD" (narrow) plus "STALE <RFC3339>" when the process sample is stale (S/view.go:303-342).
4. Animated mole (4 lines ASCII cat, `/\_/\`), 4 frames facing right, 4 mirrored, bounces across terminal width, frame step every 200ms, faster with CPU: interval = max(300 - cpu*2.5, 50) ms (S/main.go:242-250; S/view.go:62-151).
5. Cards. Width <=80: single column, each card full width-2, blank line between. Wider: two columns, column width max(24, w/2-4) passed to cards; renderTwoColumns uses cw = max(38, w/2-2); cards pair as (CPU|Memory), (Disk|Power), (Processes|Network); a dynamic stacker packs short cards beside a tall one and is used only if it makes the frame shorter (S/main.go:181-203; S/view.go:1123-1226).
6. If the frame is taller than the window it steps the core count down 0->8->4->2 (stored pref untouched); if still too tall it shows "Terminal too small. Enlarge the window to view status."; the view is padded with blank lines to full height (S/main.go:216-230,68-81).

Card chrome: `<icon> <Title>  ╌╌╌╌` (title bold #C79FD7, rule #404040). Icons: CPU ◉, Memory ◫, Disk ▥, Power ◪, Processes ❊, Network ⇅ (GPU ◧ and Sensors ◈ defined but unused) (S/view.go:41-50,959-978). Lines are cut by MaxWidth, never wrapped awkwardly.

Bars: 16 cells `█`/`░`, filled = int(pct/100*16); color by pct: >=85 red bold #FF5F5F, >=60 yellow #FFD75F, else green #A5D6A7 (S/view.go:988-1043). Battery bars are inverse (<20 red, <50 yellow) (S/view.go:1045-1054). Mini bar `▮▯` 5 cells at width<46 (S/view.go:732-735). I/O bar 5 cells, 10 MB/s per cell, >30 yellow, >80 red (S/view.go:621-631). Sparkline 8 levels `▁..█`, width min(max(cardW-22,5),16), scaled to the window max (floor 0.1), color by current rate >3 yellow, >8 red MB/s (S/view.go:790-835).

Cards:
- CPU: `Total  [bar]  NN.N%[ @ temp°C]`; the busiest N cores sorted desc as `Core%-2d [bar] NN.N%` (N=2/4/8/all, pref); or "Per-core data unavailable, using averaged load" when estimated; `Load   a / b / c, 12P+4E` or `, N cores` (S/view.go:380-428).
- Memory: `Used [bar] %`, `Free [bar] %` (avail/total), with swap: `Swap [bar] % used/total` plus `Total used / total · Avail x`; without: `Total u / t` plus `Cache x · Avail y` or `Avail`; yellow "Status warn" or red "Status critical" only if not normal (S/view.go:430-494).
- Disk: lines `INTR`/`EXTR` (numbered if several) `[bar]  100G used, 300G free` (free excludes purgeable); single disk adds `Total  500G · APFS · 30G purgeable`; red SMART line only if failing; always `I/O    ▮▮▯▯▯ R 1.2 · ▮▯▯▯▯ W 0.3 MB/s` (S/view.go:496-631). Trash is not displayed.
- Power: `Level [bar] NN.N%` (red text if <20 and not charging), `Health [bar] cap%` (<70 red, <85 yellow), `Input [bar] 96W max` when on AC, then summary `Status[ · time left] · Healthy|Fair|Service Soon · N cycles · 31.2°C · 2000 RPM`; "No battery" for desktops; placeholders `--` until the first Full (S/view.go:837-931). Card titled "Power".
- Processes: up to 3 rows `#1 [bar] 123.4%  1.2G name`, memory as RSS compact or `M12%`, name cut by display width; states "Collecting...", "No process activity", `STALE <ts>` or `UNKNOWN` warnings (S/view.go:633-717).
- Network: `Down [spark] 1.2 MB/s`, `Up [spark] ...`, then `Proxy HTTP · 192.168.1.5` or `Tunnel · ip`; IP from the default-route physical interface, else en0 (S/view.go:737-788).

Units: memory and disk via `internal/units` binary (1024) formatters: BytesBin "1.5 GB", Short "100G", Compact "1.5G"; step up so "1024.0" never prints (internal/units/bytes.go:45-93). Rates `formatRate` with rounding-aware precision (S/view.go:1070-1091). Text cut by `ansi.Truncate`, display cells (S/view.go:1105-1114; .claude/rules/go-tuis.md Status section).

Non-TTY/automation: JSON path never starts Bubble Tea (STATUS-02/03/04). Prefs file `~/.config/mole/status_prefs`, `key=value` lines, `#` comments, sorted keys, flock on `status_prefs.lock`, temp file + fsync + rename, 0644 (S/prefs.go:23-139). Keys: `cat_hidden` (true/false), `cpu_cores` (0|2|4|8; invalid -> 2).

### B.E. Dependencies (go.mod:1-45)
- `charmbracelet/bubbletea v1.3.10` (TUI loop), `lipgloss v1.1.0` (styles, layout, JoinHorizontal/Vertical), `charmbracelet/x/ansi v0.11.9` (width/truncate), `shirou/gopsutil/v4 v4.26.10` (cpu, mem, disk, net, load, host), `golang.org/x/sys` (flock in prefs, S/prefs.go:18), `cespare/xxhash` (analyze side). Indirect: termenv, colorprofile, go-runewidth/uniseg, purego (gopsutil macOS without cgo), go-ole/wmi (Windows). Internal: `internal/units` (byte formatting).
- Suggested Rust equivalents (SUGGESTION, not from the source): `ratatui` + `crossterm` for the TUI and layout, ratatui `Style` instead of lipgloss, `unicode-width` + `unicode-segmentation` for cell width and truncation, `sysinfo` for CPU/mem/swap/disk/net/uptime/load (it exposes per-core, processes, components temps), `mach2` or `libc` `host_processor_info` / `host_statistics64` for exact tick sampling and `sysctlbyname` (`hw.perflevel*`, `kern.memorystatus_vm_pressure_level`) instead of shelling out, `io-kit-sys` / `core-foundation` for AppleSmartBattery, SMC (temps/fans) and IOBlockStorageDriver stats, `battery` crate for the battery, `serde` + `serde_json` for the snapshot and NDJSON, `clap` for flags, `humantime` for durations, `fs2` or `rustix` for flock, `tokio` or std threads plus channels for off-thread collection, `windows` crate (Windows) and `/proc`, `/sys` (Linux) behind a metrics trait.

### B.F. Porting notes
1. Collection never runs in the UI loop; one `collecting` guard prevents overlap, and results carry a mode so freshness lives in the data not in timestamps (S/main.go:134-155; S/collection.go:34-37).
2. Tiered cadence: 1s cheap counters and ps; 30s for anything that spawns system_profiler, diskutil, osascript, pmset, ioreg, route, scutil; 10 min for hardware identity and GPU info; 2 min disk metadata and Finder; 30s battery and bluetooth system_profiler; 10s IPs; 5s GPU powermetrics and Trash. Native APIs would let most of this move into the fast tier, but keep the same staleness semantics.
3. Rate metrics need a primed counter and a warm collector (network primed in constructor; disk I/O returns 0 first sample; one-shot `--json` therefore shows 0 I/O). Guard against counter wrap (counterDelta) and floor elapsed (S/metrics_network.go:83-86).
4. Apple Silicon CPU: raw host_processor_info idle ticks stop for parked cores, so use max(window, busy+idle) per core, and weight the total by ticks, not the mean of per-core percentages (#1237) (S/metrics_cpu.go:314-364). Sample CPU before spawning children.
5. Do not name P/E tiers by label; use perflevel0 as the top tier (S/metrics_cpu.go:162-186).
6. Memory pressure: only the sysctl level is truthful; the `memory_pressure` tool never prints a level (#1963). macOS gopsutil Cached is 0, so File-backed pages * page size is used (page size from vm_stat header, 16384 on Apple Silicon).
7. Disk correctness: statfs counts purgeable space and local APFS snapshots as used, so Finder's figure needs osascript (TCC Automation prompt risk, hence the test-mode guard and the 2-minute failure cache) or APFSContainerFree. Share one `diskutil info -plist` per mount. Dedupe APFS volumes by base `diskN`. Do not trust capacity as device identity. External APFS can report doubled total via statfs (>1GiB mismatch rule). A native port could use `getattrlist`/`statfs` plus `NSURLVolumeAvailableCapacityForImportantUsageKey` for the Finder figure without osascript.
8. Network: ignore virtual interfaces but always keep the default-route one; tunnel counters overlap their carrier, so totals use the routed tunnel only; a `utun` alone is not proof of a proxy (label "Tunnel") (S/metrics_network.go:169-180,341-348).
9. Battery: Unknown capacity must not show "Healthy"; cycles >1000 is warn; prefer system_profiler Maximum Capacity over IORegistry estimates; ioreg prints negative int64 as uint64 two's complement; AppleRawMaxCapacity beats NominalChargeCapacity; ignore MaxCapacity (S/metrics_battery.go:150-160,272-296,503-520).
10. Not collected on macOS despite UI hooks: CPU temp, GPU temp, fan count, GPU usage without root, sensors. A native port must decide whether to use SMC (no root needed for temps/fans) and `powermetrics` alternatives (IOReport) for GPU.
11. Locale: always run children under LC_ALL=C (S/metrics.go:727-742).
12. UI: keep a placeholder for unmeasured values to avoid layout jumps (S/view.go:56-58,845-849); fit by display cells; shrink the cores shown before declaring "too small"; short-circuit to JSON when stdout is not a TTY; NDJSON watch exits when the pipe closes.
13. Process list: parse strictly and fall back to `ps aux` if the format is off; unknown state `?` is kept; zombies reported only in JSON (S/metrics_process.go:121-137).
14. Test seams worth copying: injectable `runCmd`, `commandExists`, `diskPartitionsFunc`, `diskUsageFunc`, `ioCountersFunc`, `collectProcessesFunc` (S/metrics.go:717,744; S/metrics_disk.go:47-50; S/metrics_network.go:17).
