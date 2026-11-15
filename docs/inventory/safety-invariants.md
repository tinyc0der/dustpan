# Safety invariants

Mole (tw93/Mole) deletes files for a living. This document lists the safety rules a Rust reimplementation must carry over, the never-delete lists, the deletion pipeline, dry-run parity, the privilege model, the operation log, and the incidents most likely to repeat.

## How to read the cites

- Every repository path is relative to `../mole`, branch `main`, read at HEAD `5dd93fe3`. The brief pinned `ce56a5b3`. HEAD is 62 commits later and changes `lib/core/file_ops.sh`, `lib/core/app_protection.sh` and `lib/core/base.sh`, so line numbers refer to `5dd93fe3`.
- `bugs/<file>` means `.claude/skills/bugs/references/<file>`.
- `win:<path>` means the Windows branch checkout at `a temporary extract of `origin/windows` from `../mole``.
- `I-NN` is a row of `docs/incidents.md`. A short hex string is a commit on `main`. `#NNNN` is a GitHub issue.
- Linux is unsourced. No Linux checkout was read. A Linux note is a design inference, labelled as such.
- Bash-only rules (3.2 empty arrays, errexit, `fn || handler`, `SECONDS` rounding, Bats heredoc stdin) do not transfer to Rust and are left out. Their source is `bugs/shell-and-test-pitfalls.md:29-41`.

## 1. Invariants

Each entry gives the rule, the incident or threat behind it, the enforcing source, the platforms, and a test a Rust suite can run. "All" means macOS and Windows as read, with Linux inferred.

### SAFE-01. One deletion funnel

- **Rule.** Every removal goes through the validated sink API. Raw recursive removal is allowed only for files the process created itself, and each such call carries a `# SAFE: <reason>` annotation that CI checks.
- **Why.** The validator is opt-in, so one raw `rm -rf` bypasses every layer (`docs/SECURITY_DESIGN.md:123-152`). The safety reviewer rates raw removal outside `lib/core/file_ops.sh` as P0 (`.claude/agents/safety-reviewer.md:29-32`).
- **Source.** `AGENTS.md:111`, `scripts/audit_destructive_sinks.py`, annotated sinks at `lib/core/file_ops.sh:1816-1818` and `lib/core/file_ops.sh:2240`. Windows funnel `Remove-SafeItem` at `win:lib/core/file_ops.ps1:567`.
- **Platforms.** All.
- **Test.** Deny `std::fs::remove_file`, `remove_dir`, `remove_dir_all` and `rename` outside one `sink` module with clippy `disallowed_methods`. Add a source test that fails on any such call elsewhere, and prove it red by adding one.

### SAFE-02. Reject malformed paths

- **Rule.** Refuse an empty path, a relative path, a path with `..` as a whole component, and a path with any control byte or NUL. A name such as `name..files` stays valid.
- **Why.** Relative paths resolve against an unknown working directory. Traversal and control bytes enable escapes and log forgery (`docs/SECURITY_DESIGN.md:68-70`, `docs/SECURITY_DESIGN.md:98-106`).
- **Source.** `lib/core/file_ops.sh:1164-1188`. Analyze `cmd/analyze/delete.go:677-691`.
- **Platforms.** All. Windows also refuses device and UNC paths (`win:lib/core/file_ops.ps1:70-71`).
- **Test.** Port `tests/fuzz_corpus/dangerous_paths.txt` (96 paths, `docs/SECURITY_DESIGN.md:225-232`) and assert every entry is refused. As a positive control, `/Users/a/Library/Application Support/Firefox/Profiles/x/name..files` passes the shape check. Add a `proptest` that any accepted path is absolute, NUL-free and has no `..` component.

### SAFE-03. Normalize before matching any policy

- **Rule.** Collapse `//` and `/./`, strip a trailing `/` or `/.`, and map `/System/Volumes/Data/<top>/...` to `/<top>/...` only when `/<top>` is the same directory as the Data-volume entry.
- **Why.** Equivalent spellings slipped past protection (`SECURITY_AUDIT.md:121`). Without the firmlink mapping, purge refused every path under a Data-volume root as `/System` (I-07, `docs/incidents.md:44`).
- **Source.** `lib/core/file_ops.sh:117-158`, applied at `lib/core/file_ops.sh:1190-1191`.
- **Platforms.** macOS. Windows judges both the typed and the resolved spelling (`win:lib/core/file_ops.ps1:121-158`).
- **Test.** `/Users//me/./Library/Caches/` normalizes to `/Users/me/Library/Caches`. On macOS, `/System/Volumes/Data/Users/me/x` is judged as `/Users/me/x`, and `/System/Volumes/Data/.Spotlight-V100` is refused as `/System`.

### SAFE-04. Refuse critical roots, bare containers and home roots

- **Rule.** Refuse the roots in section 2.1, including bare containers such as `/Applications` and `/Library/Application Support`, any `/Users/<name>`, and any path with the same inode as a protected root. Children of `/usr/local` and `/opt/homebrew` stay eligible. The roots themselves do not.
- **Why.** An empty variable turns `"$dir/$name"` into the parent root (`docs/SECURITY_DESIGN.md:43-47`). Case-insensitive APFS makes `/SYSTEM` the same directory as `/System` without a symlink (`lib/core/file_ops.sh:1094-1096`).
- **Source.** `lib/core/file_ops.sh:1044-1129`. Analyze `cmd/analyze/delete.go:408-483`.
- **Platforms.** All, with per-OS root lists. Windows section 2.9.
- **Test.** The check refuses `/`, `/Users/alice`, `/USERS/SHARED`, `/opt/homebrew`, `/Library/Application Support` and `/Applications`. It accepts `/Users/alice/x` and `/opt/homebrew/Cellar/foo`. A `PathBuf` built by `join("")` must never reach the sink as a root.

### SAFE-05. Resolve symlinks, deny only

- **Rule.** Re-run the deny checks on a leaf symlink's resolved target and on the path with its ancestors physically resolved, and let that re-check only deny. The resolved form can refuse a path but never grants what the literal path lacked.
- **Why.** A redirected `~/Library/Caches` would let a cache sweep walk into `~/Documents` or a system tree (`docs/SECURITY_DESIGN.md:78-96`, `SECURITY_AUDIT.md:251-259`).
- **Source.** `lib/core/file_ops.sh:1201-1274`. Analyze checks `filepath.EvalSymlinks` at `cmd/analyze/delete.go:316`. Windows refuses any linked ancestor outright (`win:lib/core/file_ops.ps1:75-80`, `win:SECURITY_AUDIT.md:48`).
- **Platforms.** All.
- **Test.** Fixture `tmp/Caches -> /System`. Candidate `tmp/Caches/x` is refused. Fixture `tmp/link -> <tempdir>/real`. Candidate `tmp/link/file` keeps the verdict of `real/file`.

### SAFE-06. Run live-state refusals before allowlist shortcuts

- **Rule.** Evaluate in this order: shape, normalize, policy loaded, leaf symlink, ancestor symlink, narrow allow (coresymbolicationd), live PowerLog database, live user cache, SQLite family, partial download, EDR cache, `/private` allowlist, critical roots, `should_protect_path`.
- **Why.** The `/private/*` allowlist returns early, so anything after it is skipped for those paths (`lib/core/file_ops.sh:1236-1237`). The 7-day power-log sweep once removed a live `CurrentXcodeOrganizerDB.XCSQL` (`AGENTS.md:122`).
- **Source.** `lib/core/file_ops.sh:1160-1384`. `docs/SECURITY_DESIGN.md:108-121` describes a simpler order (section 2.2).
- **Platforms.** macOS. The ordering principle applies to all.
- **Test.** `/private/var/db/powerlog/Library/PerfPowerTelemetry/S/CurrentFoo.SQL` is refused although `powerlog/*` is allowlisted. `.../S/ArchivedFoo.SQL` is accepted. A case variant `currentfoo.sql` is refused.

### SAFE-07. Fail closed when the policy is missing

- **Rule.** If the protection policy is not loaded, refuse every path.
- **Why.** A caller that loaded the file layer without the protection layer would otherwise skip protection (`.claude/rules/core.md:22`).
- **Source.** `lib/core/file_ops.sh:1194-1199`.
- **Platforms.** All.
- **Test.** In Rust, make the sink take `&Policy` so it cannot be built without one. Add a unit test that an empty or failed policy load returns an error, and that the sink refuses when given `Policy::unavailable()`.

### SAFE-08. Owner probes are tri-state, and unknown means busy

- **Rule.** Only a conclusive Idle answer from a process or open-file probe permits deletion, and Unknown counts as busy. Each probe returns Running, Idle or Unknown. An empty `lsof` answer counts as Idle only when the probe could see root-owned processes. One function translates the tri-state.
- **Why.** Deleting an open cache sent Autodesk Fusion's helper into a write loop that filled the disk (#1390, `SECURITY_AUDIT.md:123`). Running-owner incidents I-20, I-21, I-26, I-39 (`docs/incidents.md:57-58`, `docs/incidents.md:63`, `docs/incidents.md:76`).
- **Source.** `lib/core/base.sh:1931-1944`, `lib/core/file_ops.sh:706-768`, `lib/core/file_ops.sh:782-822`, `lib/core/file_ops.sh:904-955`, `.claude/rules/core.md:32`.
- **Platforms.** All. Windows uses a complete process snapshot plus lock files (`win:SECURITY_AUDIT.md:51`).
- **Test.** With an unreadable process table, the candidate is kept with reason "process state unknown". When `lsof` exits 1 with text on stderr, or cannot see pid 1, the state is Unknown and the candidate is kept. As a positive control, complete visibility with no match removes the candidate.

### SAFE-09. A SQLite database and its companions are one unit

- **Rule.** Keep a SQLite database and its `-wal` and `-shm` companions whole while any member is open or the probe cannot tell. Probe the base file, `-wal` and `-shm` in one call. Any open member keeps every member. A stale `-shm` alone is not liveness. Match names case-insensitively. Never unlink a WAL or SHM directly.
- **Why.** Unlinking an open database or its WAL splits live state (`SECURITY_AUDIT.md:123`, `AGENTS.md:121-122`).
- **Source.** `lib/core/file_ops.sh:253-274`, `lib/core/file_ops.sh:641-662`, `lib/core/file_ops.sh:1306-1320`, `lib/core/file_ops.sh:1683-1709`.
- **Platforms.** All.
- **Test.** Fixture `x.db`, `x.db-wal`, `x.db-shm`. While a child process holds `x.db-wal` open, deleting `x.db-shm` is refused. After the child exits, all three are accepted. `X.DB-WAL` is detected too.

### SAFE-10. Recheck at the final sink and rebind identity

- **Rule.** After sizing or any slow work, re-run the owner probe, then compare the physical parent path, the parent `(dev, ino)` and the target `(dev, ino)` with the snapshot taken before that probe. Any difference refuses. Never refresh the snapshot to approve a replacement.
- **Why.** A discovery snapshot is not an ownership lease. An owner can start, or the path can be replaced, while `du` walks it (`bugs/deletion-evidence-and-final-sink.md:60-77`, `.claude/rules/core.md:33`).
- **Source.** `lib/core/file_ops.sh:1653-1794`, `lib/core/file_ops.sh:2496-2548`, `lib/core/file_ops.sh:3282-3318`. Windows binds every ancestor to volume and file index (`win:lib/core/file_ops.ps1:32-103`).
- **Platforms.** All.
- **Test.** Plan a directory, rename it away and create a new directory at the same path before the sink. The sink refuses and logs `SKIPPED ... (identity changed)`. Untouched, it removes.

### SAFE-11. Enforce the whitelist at every sink

- **Rule.** Check the whitelist inside each sink, not only in callers. Normalize slashes. Treat a parent of a whitelisted path as protected, and a child of a literal entry as protected. Always merge the hard-safety entries. A whitelist file that exists but cannot be read stops the command.
- **Why.** Per-caller whitelist checks were missed five times (#710, #724, #738, #744, #757, `lib/core/file_ops.sh:3527-3529`). Kitty and Buzz data were lost (I-13, I-63, `docs/incidents.md:50`, `docs/incidents.md:100`).
- **Source.** `lib/core/app_protection.sh:700-756`, `lib/core/base.sh:346-559`, sinks at `lib/core/file_ops.sh:1538`, `lib/core/file_ops.sh:2043`, `lib/core/file_ops.sh:3535`.
- **Platforms.** All. Windows matches literal paths before wildcards and does not treat prefix siblings as descendants (`win:SECURITY_AUDIT.md:49`).
- **Test.** Whitelist mode 000: exit nonzero, nothing removed. Entry `~/Library/Caches/foo/bar` also protects `~/Library/Caches/foo`. A user file with one unrelated line still protects `~/Library/Caches/kitty`. `~/Library/Caches/foobar` is not protected by `~/Library/Caches/foo`.

### SAFE-12. Trash mode never falls back to permanent deletion

- **Rule.** When a Trash move fails, keep the item, log `trash-failed`, warn once, and return failure.
- **Why.** Trash is a recoverable-delete contract (`SECURITY_AUDIT.md:437`, M05).
- **Source.** `lib/core/file_ops.sh:2592-2645`. Windows has no permanent fallback for leftovers (`win:SECURITY_AUDIT.md:58`).
- **Platforms.** All.
- **Test.** Make the Trash unavailable (replace `~/.Trash` with a symlink in a fixture HOME). The item stays, `deletions.log` has `trash-failed`, and the exit status is nonzero.

### SAFE-13. The Trash destination must be safe

- **Rule.** Move only into the invoking user's Trash. The shell uses `~/.Trash`. Analyze uses `~/.Trash` on the home volume and `.Trashes/<uid>` on other volumes. Refuse a Trash that is a symlink or is owned by someone else. The shell sets the Trash to mode 700, and Analyze refuses one that group or others can write. Pick a unique name and never overwrite.
- **Why.** A replaceable Trash directory redirects a privileged move (`SECURITY_AUDIT.md:279-280`).
- **Source.** `lib/core/file_ops.sh:3033-3092`, `cmd/analyze/delete.go:189-284`.
- **Platforms.** macOS. Windows uses `ITransferSource.RecycleItem` on fixed local drives (`win:lib/core/file_ops.ps1:474-565`). Linux would follow the XDG Trash spec (inference).
- **Test.** Pre-create `~/.Trash/foo`. Trashing `foo` keeps both under different names. `~/.Trash` owned by another uid or mode 0777 is refused.

### SAFE-14. No privileged operation through a user-mutable ancestor

- **Rule.** Never run a privileged delete or move through an ancestor the invoking user can change. Walk every ancestor first. Each must be root-owned, not group or world writable, not a symlink, and not ACL-writable by the invoking user. Unknown counts as mutable. Otherwise downgrade to the user's own permissions or refuse. Never sudo-delete a symlink.
- **Why.** A directory owner can change a 0555 parent after validation and redirect a later `sudo rm` (`lib/core/file_ops.sh:1950-1953`, `AGENTS.md:118`).
- **Source.** `lib/core/file_ops.sh:1954-2006`, `lib/core/file_ops.sh:2038-2041`, `lib/core/file_ops.sh:2057-2067`, `lib/core/file_ops.sh:2449-2458`, `lib/core/file_ops.sh:2651-2655`, `lib/core/file_ops.sh:3870-3874`.
- **Platforms.** macOS, Linux (inference).
- **Test.** A target under a user-owned directory with `needs_privilege = true`. A recording privilege runner shows no privileged command ran, and the result is a downgrade or `MutableParent`.

### SAFE-15. Privileged Trash moves cross a root-owned stage

- **Rule.** Move the item by same-volume rename into a fresh `mktemp` directory under root-owned 0711 `/Library/MoleTrashStaging`, hand ownership to the user with `chown -Rhx`, then move it into Trash as the user. On failure, keep the payload in the stage and print its path. Root never touches the stage again after the handoff.
- **Why.** `~/.Trash` is user-controlled and can be replaced after validation (`lib/core/file_ops.sh:3115-3119`, `.claude/rules/core.md:26`).
- **Source.** `lib/core/file_ops.sh:2932-2981`, `lib/core/file_ops.sh:3115-3230`.
- **Platforms.** macOS.
- **Test.** At unit level, a cross-volume source is refused before any move. A failure after staging reports the exact stage path and leaves the payload there. Real staging needs a root-capable macOS runner, so gate that test.

### SAFE-16. Never run as root

- **Rule.** Exit at startup when the effective uid is 0. Request elevation per operation.
- **Why.** User state, package managers and cache initialization must not run as root (`AGENTS.md:117`).
- **Source.** `mole:8-13`, `bin/clean.sh:8-13`, `bin/touchid.sh:11`.
- **Platforms.** macOS, Linux (inference). Windows runs with the caller's token and checks admin per task (`win:SECURITY_AUDIT.md:72`).
- **Test.** With an injected `euid() == 0`, the binary exits 1 with the fixed message and touches no file.

### SAFE-17. Test seams never steer a real run

- **Rule.** An override that picks a deletion root, a Trash target or a privileged write target is honored only in test mode.
- **Why.** Seven test seams, two of them deleted through sudo, were honored in real runs (I-64, `docs/incidents.md:101`).
- **Source.** `lib/core/file_ops.sh:2829-2840`, `lib/core/file_ops.sh:2853-2858`, `lib/core/sudo.sh:12-22`, `AGENTS.md:115`, `.claude/rules/core.md:24`.
- **Platforms.** All.
- **Test.** Put seams behind `#[cfg(test)]` or a test-only feature, not environment variables. A release-build test sets the old variable names and asserts the default roots are used.

### SAFE-18. Dry run and real run share one eligible plan

- **Rule.** Every eligibility check runs before the dry-run branch. Only the action differs. Section 4 lists the rules and the intended differences.
- **Why.** Protection and an active-process guard once ran only in real mode, so previews promised work the real run refused (`bugs/deletion-evidence-and-final-sink.md:53-58`). I-10 recorded dry-run targets twice (`docs/incidents.md:47`).
- **Source.** `lib/core/file_ops.sh:1548-1557`, `lib/core/file_ops.sh:2049-2055`, `bin/clean.sh:238-299`.
- **Platforms.** All.
- **Test.** Run the planner over one fixture tree in dry mode and in real mode with a recording sink. Assert the same candidate set and the same refusal reasons.

### SAFE-19. Bound every external command and use only complete output

- **Rule.** Give every subprocess a deadline and null stdin, and act on a producer's output only after it exits 0. Probes, sizing and moves each have a deadline. Children also get non-interactive flags such as `sudo -n` and `mv -n`.
- **Why.** Process substitution once fed a timed-out `find` prefix into deletion (`lib/core/file_ops.sh:3798-3801`). A sandbox cache step hung for minutes (I-71, `docs/incidents.md:108`). BSD `mv` can prompt on stdin (`bugs/shell-and-test-pitfalls.md:43-47`).
- **Source.** `lib/core/timeouts.sh:61-72`, `lib/core/file_ops.sh:3511-3526`, `lib/core/file_ops.sh:3580-3597`, `bugs/shell-and-test-pitfalls.md:5-19`.
- **Platforms.** All.
- **Test.** A fake `find` prints two paths, then sleeps past the deadline. Nothing is deleted. Every deadline constant is at least 2 seconds.

### SAFE-20. Classify every timeout and signal by contract

- **Rule.** Treat a signal as a sticky cancel, a safety-probe timeout as Unknown, a sizing timeout as a partial total, a removal timeout as one failed item, and a section budget as a stop for that section only. After a signal, no later probe or sink runs. A safety-probe timeout is Unknown and keeps that item. A sizing timeout keeps the item eligible and marks the total partial. A removal timeout is one failed item, and later items continue. A section budget stops that section only. An owner command's errno-style high exit code is an ordinary failure, not a signal.
- **Why.** A PATH `timeout` shim reported 143, so every slow scan read as Ctrl-C (I-61, `docs/incidents.md:98`). The inverse defect deletes with unknown evidence (`bugs/shell-and-test-pitfalls.md:64-97`).
- **Source.** `lib/core/timeout.sh:104-117`, `lib/core/file_ops.sh:1453-1464`, `lib/core/file_ops.sh:1496-1499`, `lib/core/file_ops.sh:1822-1844`, `lib/core/file_ops.sh:4199-4211`.
- **Platforms.** All.
- **Test.** Model `enum Probe { Yes, No, Unknown, Cancelled }` and a sticky `CancelToken`. The first candidate's guard returns `Cancelled`, and the second candidate would be eligible. Assert the second probe and sink never run. As a positive control, the second candidate is removed when no cancel occurs.

### SAFE-21. Count only completed work, and never invent a size

- **Rule.** Count only completed mutations, and never record an unknown size as 0. Refused, filtered, failed and timed-out items add no items and no bytes. Unknown size is `unknown`, never 0. A sizing timeout prints the total as `N+`. Size is allocated bytes. Internal hardlinks count once only when every link is inside the candidate. Sparse files and clones do not count by logical length. Symlinks are not followed. Measurement stops at a device boundary.
- **Why.** Six simulators holding 7.73 GB were counted as 0 (`.claude/rules/core.md:27`). Two paths that compute one number drift (`bugs/state-accounting-and-progress.md:23-40`).
- **Source.** `bin/clean.sh:747-797`, `lib/core/file_ops.sh:4085-4182`, `lib/core/file_ops.sh:4183-4211`, `lib/core/file_ops.sh:2460-2463`.
- **Platforms.** All. Windows does not count recycled bytes as freed (`win:SECURITY_AUDIT.md:58`).
- **Test.** A 1 GB sparse file with 4 KB allocated reports about 4 KB. A refused item leaves the totals unchanged. A forced sizing timeout prints `+`.

### SAFE-22. Log every mutation and every refusal

- **Rule.** Append one `operations.log` row per removal, Trash move, refusal and failure, synchronously. Escape control bytes. A failed append warns once and does not abort.
- **Why.** Support reads these logs after a reported deletion. A silent write failure leaves no trail (`lib/core/log.sh:50-53`). Control bytes in names could forge records (`AGENTS.md:132`).
- **Source.** `lib/core/log.sh:61-94`, `lib/core/log.sh:288-309`, `lib/core/base.sh:805-830`, `lib/core/file_ops.sh:3430-3462`, `AGENTS.md:133`.
- **Platforms.** All. Windows writes no rows in dry run (`win:lib/core/log.ps1:33-35`). Mole leaves one gap that Rust should close. A failed move in the uninstall Trash batch leaves only a debug line (`lib/uninstall/batch.sh:977-981`).
- **Test.** A file name holding a newline produces exactly one row containing a literal `\n`. A read-only log directory prints one warning on stderr, and the deletion still happens.

### SAFE-23. Leftovers need exact ownership evidence

- **Rule.** Claim app data only by exact bundle id with a dot boundary, or by exact app path. Never by vendor prefix, Team ID, display-name glob, substring or fallback wildcard. Skip name matching for generic names. Never claim a shared root.
- **Why.** Uninstalling Claude.app removed Claude Code CLI state (#993, `SECURITY_AUDIT.md:304`). `com.foo*` matched `com.foobar.plist`. Substring teardown removed `Foo-beta.app`. Team ID fallbacks were merged and reverted (`bugs/deletion-evidence-and-final-sink.md:5-15`).
- **Source.** `AGENTS.md:60`, `AGENTS.md:134`, `lib/core/base.sh:1138-1170`, `lib/core/app_protection.sh:347-379`, `lib/core/app_protection_data.sh:644-650`.
- **Platforms.** All.
- **Test.** Installed `com.foo`. `~/Library/Preferences/com.foobar.plist` is not claimed. `~/.config/claude` is not claimed by `Claude.app`. An app named `Helper` gets no name-based LaunchAgent match.

### SAFE-24. Prove that no sibling install exists

- **Rule.** Remove shared leftovers only after a complete scan proves that no other install of the same family exists. The family is the same id, a dot-continuation id, or a channel-stripped name. The answer has three states, and only a complete "none" unlocks shared leftovers. Recheck after the app moves. A reappeared app path or an incomplete inventory keeps the whole family.
- **Why.** A Nightly build's uninstall offered the stable app's folders (I-24, `docs/incidents.md:61`). Absence must be proven, not assumed (`SECURITY_AUDIT.md:125`).
- **Source.** `lib/uninstall/batch.sh:770-800`, `lib/uninstall/batch.sh:1469`, `lib/uninstall/batch.sh:1765-1806`, `.claude/rules/uninstall.md:49-57`.
- **Platforms.** All. Windows requires the registration gone and a complete inventory (`win:SECURITY_AUDIT.md:56`).
- **Test.** `Zed.app` and `Zed Nightly.app` installed. Uninstalling Nightly keeps the stable folders. An unreadable volume yields Unknown and keeps shared leftovers with reason `owner-unknown`.

### SAFE-25. Cached evidence is keyed to the evidence

- **Rule.** A cached answer that can authorize deletion is keyed by a fingerprint of all inputs that could change it, never by age alone. A timestamp from the future counts as expired for freshness checks and as young for deletion-age checks.
- **Why.** A one-hour receipt cache was accepted as proof that no sibling existed (`bugs/state-accounting-and-progress.md:19-21`). Clock corrections kept stale stamps valid (`.claude/rules/core.md:23`).
- **Source.** `bugs/state-accounting-and-progress.md:5-21`, `.claude/rules/core.md:23`, `SECURITY_AUDIT.md:125`.
- **Platforms.** All.
- **Test.** The cache says no sibling. Add a new receipt to the fixture. The next query misses the cache and finds the sibling. A stamp one hour in the future triggers a refresh.

### SAFE-26. Age comes from the newest content

- **Rule.** An age threshold reads the newest file inside the tree, at discovery and again at the sink. A walk that fails or times out keeps the item.
- **Why.** Directory mtime missed writes to existing files, so active folders read as 30 days idle (I-46, I-47, `docs/incidents.md:83-84`).
- **Source.** `.claude/rules/clean.md:74-75`, `SECURITY_AUDIT.md:438-439`.
- **Platforms.** All.
- **Test.** A directory with mtime 60 days ago and one file inside written today is kept. A walk that exceeds its budget keeps the directory.

### SAFE-27. Protect ancestors of protected state and structural containers

- **Rule.** A whole-directory delete or move is refused when a protected or separately guarded path lies inside it. Removing a structural container is different from emptying it.
- **Why.** Moving `Library/Containers` took Docker and OrbStack state (`SECURITY_AUDIT.md:34`). The generic sweep removed `~/Library/Caches/Google` whole (I-37). `mo clean` removed `DiagnosticReports` itself (I-03). Analyze could move iCloud stores (I-27). See `docs/incidents.md:40`, `docs/incidents.md:64`, `docs/incidents.md:74`.
- **Source.** `cmd/analyze/delete.go:356-404`, `cmd/analyze/delete.go:485-560`, `lib/core/app_protection.sh:425-427`.
- **Platforms.** All. Windows inspects protected descendants before Analyze deletes (`win:SECURITY_AUDIT.md:59`).
- **Test.** Deleting `~/Library/Group Containers` while `x.dev.orbstack` exists inside is refused. With it absent, the delete is allowed. Cleaning `~/Library/Logs/DiagnosticReports` leaves the directory.

### SAFE-28. Cache-shaped names are not evidence

- **Rule.** Folders under a `Caches` parent that hold installed binaries, models, dependencies, sessions or transcripts are protected through a denylist overlay and hard-safety whitelist entries.
- **Why.** Camoufox, Cypress, Puppeteer, Buzz, npx, Yarn Berry and Claude Code transcripts were deleted (I-42, I-45, I-48, I-63, I-11, I-22, I-73 at `docs/incidents.md:79`, `docs/incidents.md:82`, `docs/incidents.md:85`, `docs/incidents.md:100`, `docs/incidents.md:48`, `docs/incidents.md:59`, `docs/incidents.md:110`).
- **Source.** `lib/core/app_protection.sh:410-423`, `lib/core/app_protection.sh:560-574`, `lib/core/app_protection.sh:587-636`, `lib/core/base.sh:199-233`, `.claude/rules/clean.md:28`.
- **Platforms.** All.
- **Test.** `~/Library/Caches/Cypress/13.0.0/Cypress.app` survives the generic Caches sweep with the default whitelist and with a custom one.

### SAFE-29. Resolve sandbox well-known folders physically

- **Rule.** Never treat a container's well-known user folders as app cache. Inside an app container, `Data/Downloads`, `Desktop`, `Pictures`, `Music` and `Movies` are usually links to the user's folders, and `Data/Documents` is user data. Only `Data/Library/Caches` and `Data/tmp` are rebuildable.
- **Why.** A request to clean WPS's `Data/Downloads/*` would have emptied the real `~/Downloads` (#1578, `bugs/deletion-evidence-and-final-sink.md:103-121`).
- **Source.** `lib/core/app_protection.sh:495-511`.
- **Platforms.** macOS.
- **Test.** A container whose `Data/Downloads` links to a fixture Downloads with files produces no candidate.

### SAFE-30. Discovery never enters a deletion target

- **Rule.** Artifact names such as `node_modules`, `vendor` and `Pods` are never accepted as project containers during recursive discovery.
- **Why.** A stray `~/node_modules` made each package a project, and package-internal `dist` and `build` reached the delete list (#1459, `bugs/deletion-evidence-and-final-sink.md:79-85`).
- **Source.** `lib/clean/project.sh:40`, `lib/clean/project.sh:1457`.
- **Platforms.** All.
- **Test.** `~/node_modules/pkg/package.json` plus `pkg/dist/` never yields `pkg/dist`. An explicit configured root still reaches it.

### SAFE-31. Owner metadata is not a deletion inventory

- **Rule.** A journal, registry, cache or Git index can narrow a candidate but never authorizes it alone. Recheck tracked content and nested repositories at the sink.
- **Why.** `mo clean` removed project caches that Git tracked (I-02, `docs/incidents.md:39`). A reconstructed keep-set can miss a new owner reference (`bugs/deletion-evidence-and-final-sink.md:87-101`).
- **Source.** `.claude/rules/clean.md:85`.
- **Platforms.** All.
- **Test.** A `.next/cache` directory with one Git-tracked file is kept. An untracked sibling is removed.

### SAFE-32. A third-party cleanup command is a sink too

- **Rule.** Treat a third-party cleanup command as a deletion sink. Before running `brew`, `npm`, `go clean`, `simctl` or a similar owner command, preview its exact candidates, resolve and validate its physical roots against protection and whitelist, and share one plan between dry and real runs. Never fall back to direct deletion when the owner command fails.
- **Why.** A configured symlink let an owner command bypass a whitelisted physical root (`SECURITY_AUDIT.md:35`). npm exits 243, which is not a signal (#1695, `bugs/shell-and-test-pitfalls.md:97`).
- **Source.** `.claude/rules/clean.md:29`, `lib/core/timeout.sh:112-117`.
- **Platforms.** All. Windows owner commands follow the same rule (`win:SECURITY_AUDIT.md:50`).
- **Test.** When an owner cache root resolves through a symlink into a whitelisted directory, the owner command is not invoked.

### SAFE-33. Kill processes only by literal, non-system names

- **Rule.** Escape a process name before matching, reject empty or control-byte names, and refuse system process names before any quit or kill step.
- **Why.** `CFBundleExecutable` is set by the app, and an unescaped `.*` matches nearly every process (`AGENTS.md:130`, `SECURITY_AUDIT.md:301`).
- **Source.** `lib/core/app_protection.sh:2714-2733`, `lib/core/app_protection.sh:2777-2788`.
- **Platforms.** All.
- **Test.** An executable named `Finder` is refused. A name `a.*b` matches only the literal `a.*b`.

### SAFE-34. Confirmation needs a real terminal

- **Rule.** A destructive confirmation reads from a TTY. A failed or empty read is not Enter. A dry run without a TTY prints the plan and exits 0.
- **Why.** A piped `n` or `/dev/null` must not confirm a removal (`.claude/rules/uninstall.md:35`).
- **Source.** `lib/uninstall/batch.sh:2516-2544`.
- **Platforms.** All.
- **Test.** Real mode with stdin from `/dev/null` exits 1 and removes nothing. The same in dry run exits 0 and prints the plan.

### SAFE-35. Exit status and refusal messages tell the truth

- **Rule.** Exit nonzero when any removal fails, refuse up front when the process cannot write, and name a cause and a next action for each refusal. The next action must fail on the broken state.
- **Why.** `mo uninstall` exited 0 after a failure (I-25). `mo clean` in a read-only shell printed success (I-53). See `docs/incidents.md:62` and `docs/incidents.md:90`. Gate rules are in `AGENTS.md:127` and `bugs/test-validity-and-refusal-diagnostics.md:51-89`.
- **Source.** `bin/clean.sh:1500-1560`, `AGENTS.md:127`.
- **Platforms.** All.
- **Test.** A target whose parent is mode 0555: exit nonzero and the summary does not claim completion. Each refusal reason maps to a stable code.

### SAFE-36. Parse only machine formats

- **Rule.** Read structured output (plist XML, JSON, `-F` fields) with `LC_ALL=C`. Validate field shape. Join records on identifiers, never on display strings.
- **Why.** `plutil -p` is depth-blind, so a nested key offered a directory for deletion (I-01, `docs/incidents.md:38`). A name join called every simulator point release an orphan (#1505).
- **Source.** `AGENTS.md:129`, `bugs/shell-and-test-pitfalls.md:49-62`.
- **Platforms.** All.
- **Test.** A plist fixture `{"outer":{"inner":true}}` never yields `inner` as a candidate.

### SAFE-37. Some surfaces stay read-only

- **Rule.** Never delete Software Update staging (`/Library/Updates`, `/macOS Install Data`), rebuild LaunchServices with `lsregister -r`, delete Dock databases, or vacuum a live PowerLog database. `mo history` never writes.
- **Why.** Age and process lists cannot prove those trees idle (`AGENTS.md:119`). A LaunchServices rebuild made a running VPN read as reinstalled (`SECURITY_AUDIT.md:124`). Dock repair reset wallpaper (`SECURITY_AUDIT.md:347`).
- **Source.** `AGENTS.md:119`, `AGENTS.md:122`, `SECURITY_AUDIT.md:124`, `SECURITY_AUDIT.md:329`.
- **Platforms.** macOS.
- **Test.** The planner emits no candidate for fixture `/Library/Updates/x`. A source test fails on any `lsregister -r` string outside comments.

### SAFE-38. Respect mount, cloud and network boundaries

- **Rule.** Stop sizing and scanning at device boundaries, never list a dataless (online-only) folder, and skip tools that open every mounted volume while a network share is mounted.
- **Why.** Analyze counted mounted simulator runtimes as 57 GB on a 7 GB disk (I-69, I-75, `docs/incidents.md:106`, `docs/incidents.md:112`). Listing a dataless CloudStorage folder downloads it (`bugs/shell-and-test-pitfalls.md:104`).
- **Source.** `bin/clean.sh:776`, `lib/core/file_ops.sh:3186`, `lib/clean/project.sh:1036-1105`, `lib/clean/user.sh:1978`, `lib/clean/snapshots.sh:67`, `.claude/rules/clean.md:54`.
- **Platforms.** All for device boundaries. macOS for dataless and tmutil rules.
- **Test.** A disk image mounted inside the scan root is excluded from the size. The recursive delete boundary is open (see Unresolved).

### SAFE-39. Call system tools by absolute path

- **Rule.** When shelling out, call the OS tool by absolute path (`/usr/bin/du`, `/usr/sbin/sysctl`, `/usr/bin/stat`), not through `PATH`.
- **Why.** GNU coreutils first in `PATH` changed `stat -f` output and aborted `mo clean` (#1752). A `PATH` without `/usr/sbin` read memory as 0 and refused `mo slim` (I-06, `docs/incidents.md:43`).
- **Source.** `.claude/rules/shell.md:16-23`.
- **Platforms.** All, with per-OS paths.
- **Test.** Put a fake `du` that prints garbage first on `PATH`. Measurements are unchanged.


## 2. The never-delete list

Mole layers four independent refusals. A path must pass all of them. The layers are path shape, critical roots, live state, and protection policy. The whitelist is a fifth, user-controlled layer.

### 2.1 Critical deletion roots, every command

The source is `_mole_is_critical_deletion_path` at `lib/core/file_ops.sh:1044-1129`. The validator calls it on the normalized path (`lib/core/file_ops.sh:1367`), on a leaf symlink's target (`lib/core/file_ops.sh:1220`), and on the physically resolved path when an ancestor is a symlink (`lib/core/file_ops.sh:1262`).

| Arm | Match | Lines |
|---|---|---|
| `/usr/local/*`, `/opt/homebrew/*` | Glob. Exempt, children stay deletable. The roots themselves fall through. | `lib/core/file_ops.sh:1051-1053` |
| `/`, `/Library`, `/Library/Application Support`, `/Applications`, `/Volumes`, `/opt`, `/opt/homebrew`, `/Users`, `/Users/Shared`, `/private`, `/private/tmp`, `/var`, `/var/root`, `/private/var`, `/private/var/tmp`, `/private/var/folders`, `/private/var/root` | Exact string. The root is refused, its children are judged by later rules. | `lib/core/file_ops.sh:1054-1081` |
| `/bin`, `/dev`, `/sbin`, `/usr`, `/System`, `/Library/Apple`, `/Library/Extensions`, `/Library/Keychains`, `/Applications/Finder.app`, `/Applications/Safari.app`, `/Users/Guest`, `/etc`, `/private/etc`, `/var/db`, `/var/audit`, `/private/var/db`, `/private/var/audit` | Exact or any descendant. | `lib/core/file_ops.sh:1054-1081` |
| `/Users/<name>` | Exactly one component below `/Users`. Blocks the empty-variable collapse. | `lib/core/file_ops.sh:1085-1092` |
| 19 exact roots (adds `/Network`, `/cores`, `/home`, `/net`, `/tmp`) | Same inode (`-ef`), so `/SYSTEM` or `/OPT/HOMEBREW` on case-insensitive APFS match. | `lib/core/file_ops.sh:1094-1107` |
| 14 protected trees | The path or any existing ancestor has the same inode as the tree root. | `lib/core/file_ops.sh:1109-1118` |
| Any direct child of `/Users` | The parent has the same inode as `/Users`. | `lib/core/file_ops.sh:1120-1126` |

Paths under `/System/Volumes/Data/<top>` are judged as `/<top>` only when `/<top>` is the same directory as the Data-volume entry (`lib/core/file_ops.sh:139-158`, I-07).

### 2.2 Carve-outs that the validator accepts early

`validate_path_for_deletion` returns success for these before it reaches the critical-root and protection checks (`lib/core/file_ops.sh:1276-1281`, `lib/core/file_ops.sh:1351-1364`):

- `/System/Library/Caches/com.apple.coresymbolicationd/data` and its descendants.
- Descendants of `/private/tmp`, `/private/var/tmp` and `/private/var/folders`.
- `/private/var/log`, `/private/var/db/diagnostics`, `/private/var/db/DiagnosticPipeline`, `/private/var/db/powerlog`, `/private/var/db/reportmemoryexception`, each with its descendants.
- `/private/var/db/receipts/*.bom` and `/private/var/db/receipts/*.plist`.

A direct child of the invoking user's `~/.Trash` skips `should_protect_path`, because emptying Trash is an explicit discard (`lib/core/file_ops.sh:1131-1147`, `lib/core/file_ops.sh:1373-1374`). It still passes the critical-root check.

The docs drift from the code. `docs/SECURITY_DESIGN.md:108-121` describes "allow-then-deny". The code runs symlink, ancestor, PowerLog, live-cache, SQLite, incomplete-download and EDR refusals before the allowlist (`lib/core/file_ops.sh:1201-1349`). A reimplementation must follow the code order.

### 2.3 Live-state refusals inside the validator

These refuse even inside an allowlisted subtree:

- The live PowerLog database, meaning any `Current*` file one level below `/private/var/db/powerlog/Library/PerfPowerTelemetry/<store>/`, plus `-wal` and `-shm`, matched case-insensitively (`lib/core/file_ops.sh:160-192`, `lib/core/file_ops.sh:1283-1289`). Rotated `Archived*` files stay eligible.
- A reverse-DNS directory under `~/Library/Caches`, `~/Library/Containers/<id>/Data/Library/Caches` or `~/Library/Group Containers/<id>/(Library/)Caches` whose owner runs or whose state is unknown (`lib/core/file_ops.sh:206-248`, `lib/core/file_ops.sh:904-955`).
- Any `*.db`, `*.sqlite` or `*.sqlite3` file, or its `-wal`, `-shm` or `-journal` companion, case-insensitive, while any family member is open or the probe cannot tell (`lib/core/file_ops.sh:253-274`, `lib/core/file_ops.sh:1306-1320`).
- `~/Downloads/*.download`, `*.crdownload`, `*.part` and `~/Library/Caches/Homebrew/downloads/*.incomplete`, one level deep, while open or unknown (`lib/core/file_ops.sh:289-302`, `lib/core/file_ops.sh:1322-1337`).
- Anything under `/private/var/folders` whose path contains an EDR vendor prefix (`lib/core/file_ops.sh:1339-1349`).

### 2.4 Protection policy: `should_protect_path`

The source is `lib/core/app_protection.sh:406-695`. The function is mode-aware. `MOLE_UNINSTALL_MODE=1` relaxes data protection for an app the user chose to uninstall but keeps system-critical protection. Steps in order:

| Step | What it protects | Match | Lines |
|---|---|---|---|
| Clean-only state | `~/.prometheus/data`, `~/.jupyter/runtime`, `~/.yarn/berry/cache`, `~/.gitconfig.lock`, `~/.gitconfig.bak*`, shell history backups | Exact or descendant glob | `lib/core/app_protection.sh:410-423` |
| Shared home roots | `~/.cache`, `~/.config`, `~/.local`, `~/.local/{bin,lib,share,state}`, `~/Library/Unity`, `~/Library/Application Support/Epic` | Exact root only, case-insensitive character classes | `lib/core/app_protection.sh:347-362`, `lib/core/app_protection.sh:425-427` |
| Steam library | `~/Library/Application Support/Steam` and descendants | Uninstall mode only, case-insensitive | `lib/core/app_protection.sh:370-379`, `lib/core/app_protection.sh:429-431` |
| OrbStack runtime | `Group Containers/*dev.orbstack`, `~/.orbstack` and descendants | nocasematch glob | `lib/core/app_protection.sh:301-318`, `lib/core/app_protection.sh:450-452` |
| System Settings, Control Center, Notes | Substring keywords | Glob with character classes | `lib/core/app_protection.sh:454-465` |
| UI-rendering caches and Apple containers | Settings, Finder, Dock caches and containers, `com.apple.sharedfilelist` entries | Substring glob | `lib/core/app_protection.sh:467-493` |
| Container and Group Container interiors | The container's bundle id goes through `should_protect_data`, except `Data/Library/Caches/*`, `Data/tmp/*` and three WeChat cache leaves | Regex extraction, then bundle policy | `lib/core/app_protection.sh:495-511` |
| Hardcoded Apple components | `com.apple.Settings`, `SystemSettings`, `controlcenter`, `finder`, `dock` | Substring glob | `lib/core/app_protection.sh:513-518` |
| EDR caches | Vendor prefix under `/private/var/folders` | Substring, nocasematch | `lib/core/app_protection.sh:520-525`, `lib/core/app_protection.sh:271-299` |
| Named user data | Dock and Finder prefs, Mole's own logs, Codex state, Claude Code transcripts (`~/.claude`, `projects`, `file-history`, `history.jsonl`), ByHost Bluetooth and Wi-Fi, NetworkExtension prefs, `Mobile Documents`, `Library/Accounts`, `Keychains`, `Mail`, `Calendars`, `Contacts`, audio plug-ins, iZotope, LaserSoft, licence prefs, Playwright, Camoufox, Cypress, Puppeteer, Adobe caches, HomeKit, FamilyCircle, Shortcuts sandboxes, Google ID store, idleassetsd and wallpaper, CoreAudio | Exact or descendant glob, some case-insensitive | `lib/core/app_protection.sh:527-655` |
| Full-path bundle glob | Clean mode: one `@(a\|b\|...)` alternation of `SYSTEM_CRITICAL_BUNDLES` plus `DATA_PROTECTED_BUNDLES`. Uninstall mode: `APPLE_UNINSTALLABLE_APPS` first (returns eligible), then `SYSTEM_CRITICAL_BUNDLES` | Whole-string glob against the full path | `lib/core/app_protection.sh:657-682`, `lib/core/app_protection.sh:33-58` |
| Basename | Clean mode only. The last path component goes through `should_protect_data` | Inline globs, then the data list | `lib/core/app_protection.sh:684-691` |

The full-path glob compares the whole path against each pattern, so only the 47 patterns that start with `*` can match there (inference from `lib/core/app_protection.sh:52-58`). Plain bundle ids such as `com.apple.finder` protect a path through the basename step and the `com.apple.*` blanket in `should_protect_data` (`lib/core/app_protection.sh:199-205`). `SYSTEM_CRITICAL_BUNDLES` has no entry that starts with `*`, so in uninstall mode the full-path loop at `lib/core/app_protection.sh:671-676` cannot match an absolute path with the shipped list (inference). Uninstall gets its system-app protection from `should_protect_from_uninstall` on the bundle id instead (`lib/core/app_protection.sh:143-157`). A Rust design should not treat that list as path protection.

### 2.5 Bundle and name lists

All in `lib/core/app_protection_data.sh`, a data-only file:

| List | Lines | Entries | Match | Used by |
|---|---|---|---|---|
| `SYSTEM_CRITICAL_BUNDLES` | `lib/core/app_protection_data.sh:19-160` | 129 | Case-sensitive glob. For uninstall, converted to an anchored regex (`.` escaped, `*` becomes `.*`) at `lib/core/app_protection.sh:109-157` | Uninstall refusal, clean path policy |
| `APPLE_UNINSTALLABLE_APPS` | `lib/core/app_protection_data.sh:163-175` | 11 | Same, checked first so Xcode, Final Cut, Logic, iWork and similar stay uninstallable | `should_protect_from_uninstall`, uninstall-mode path policy |
| `OFFICIAL_UNINSTALLER_RULES` | `lib/core/app_protection_data.sh:179-203` | 17 rows | `vendor\|bundle-prefixes\|name-fragments`. Prefix or lowercase substring | Routes EDR, MDM, antivirus apps to the vendor uninstaller (`lib/core/app_protection.sh:161-196`) |
| `ENDPOINT_SECURITY_BUNDLE_PREFIXES` | `lib/core/app_protection_data.sh:215-226` | 10 | Case-insensitive substring under `/private/var/folders` only | EDR cache refusal |
| `DATA_PROTECTED_BUNDLES` | `lib/core/app_protection_data.sh:229-642` | 353 | Case-sensitive glob. 47 start with `*` | Clean-mode data protection |
| `LAUNCH_AGENT_NAME_COMMON_WORDS` | `lib/core/app_protection_data.sh:644-650` | 40 words | Exact display name | Disables name-based LaunchAgent matching for generic names |

`should_protect_data` adds inline families before the list: `com.apple.*`, `org.cups.*`, keychain, security, bluetooth, wifi, network, TCC, input methods, keyboard layouts, Karabiner, password managers, JetBrains, Microsoft, Sublime, Cursor, Claude, ChatGPT, Codex, Ollama, proxy and VPN clients, Docker, Postman, Insomnia (`lib/core/app_protection.sh:199-258`). Pattern entries must not contain `|`, `(` or `)`, and quoting the right-hand side would silently turn every wildcard into an exact match (`.claude/rules/uninstall.md:23`).

### 2.6 Whitelist entries that a user cannot remove

`SAFETY_WHITELIST_PATTERNS` always merge into a user whitelist (`lib/core/base.sh:194-233`, merge at `lib/core/base.sh:346-381`). The entries are `FINDER_METADATA`, and under `~/Library/Caches` the folders `com.apple.FontRegistry*`, `com.apple.spotlight*`, `com.apple.Spotlight*`, `CloudKit*`, `kitty`, `pypoetry/virtualenvs*`, the renv cache, `pnpm/dlx*`, `llama.cpp` and `Buzz`. `DEFAULT_WHITELIST_PATTERNS` (`lib/core/base.sh:172-189`) apply only when no user file exists and are replaceable.

### 2.7 Analyze has its own list

`mo analyze` is Go and does not call `file_ops.sh` (`SECURITY_AUDIT.md:208`). Its refusals (`cmd/analyze/delete.go:322-483`):

- 34 exact critical roots, by string or same inode (`cmd/analyze/delete.go:408-449`).
- Any direct child of `/Users` (`cmd/analyze/delete.go:451-456`).
- 14 protected trees and their descendants (`cmd/analyze/delete.go:458-481`).
- Every direct child of `/Library` and `~/Library` (`cmd/analyze/delete.go:338-340`, `cmd/analyze/delete.go:502-510`).
- The home root and `Library`, `Documents`, `Desktop`, `Downloads`, `Pictures`, `Movies`, `Music` themselves, case-insensitive (`cmd/analyze/delete.go:485-500`).
- Docker Desktop and OrbStack state and any ancestor that contains it (`cmd/analyze/delete.go:356-404`).
- iCloud structural folders and their ancestors (`cmd/analyze/delete.go:374-376`, `cmd/analyze/delete.go:512-560`).
- EDR caches, checked before HOME is consulted (`cmd/analyze/delete.go:329-334`).

### 2.8 Processes never signalled

`force_kill_app` refuses `Finder`, `Dock`, `loginwindow`, `WindowServer`, `SystemUIServer`, `launchd`, `coreaudiod`, `NotificationCenter`, `ControlCenter` and `Spotlight` by exact name, before any quit or kill (`lib/core/app_protection.sh:2777-2788`).

### 2.9 Windows equivalents

Protected prefixes (`C:\Windows`, `System32`, `SysWOW64`, both Program Files, Windows Defender roots, `%SYSTEMROOT%`, `%WINDIR%`), the exact user-profile root, and protected path segments (`Microsoft\Credentials`, `Protect`, `Crypto`, `Vault`, `SystemCertificates`, `Windows\Start Menu`) matched case-insensitively as whole segments (`win:lib/core/base.ps1:184-212`). Clean adds credential, browser-profile and transcript segments (`win:lib/core/base.ps1:214-248`). Non-drive-letter paths (device and UNC) are never cleanup roots (`win:lib/core/file_ops.ps1:70-71`).

## 3. The deletion pipeline

The bugs skill names the lifecycle every destructive family must follow (`.claude/skills/bugs/SKILL.md:39-65`): discover, cheap filters, owner probes, sizing, final owner re-probe, identity rebind, sink, accounting. The final-sink matrix states the evidence each stage needs (`bugs/deletion-evidence-and-final-sink.md:60-77`).

### 3.1 `mo clean`

1. The entry refuses root (`bin/clean.sh:8-13`), loads the whitelist and stops if it cannot read it (`bin/clean.sh:47`, `lib/core/base.sh:473-559`), and starts an operations-log session (`bin/clean.sh:1575`).
2. `start_cleanup` settles sudo once, before any section runs (`bin/clean.sh:1561-1640`). This order is load-bearing because the lsof visibility mode is memoized on first use (`AGENTS.md:125`).
3. Section producers in `lib/clean/*.sh` discover candidates. A scan writes to a temp file under a deadline and is discarded on any nonzero status (`lib/core/file_ops.sh:3511-3526`, privileged form `lib/core/file_ops.sh:3580-3597`).
4. `_safe_clean_impl` drops missing targets, then protected (`bin/clean.sh:933`), whitelisted (`bin/clean.sh:941`, logged `SKIPPED whitelist`), and compiled-model caches (`bin/clean.sh:948`), before any owner probe or preview registration. Nested paths are collapsed (`bin/clean.sh:706`).
5. Sizing uses allocated blocks, stops at a device change, and counts internal hardlinks once (`bin/clean.sh:747-797`, `lib/core/file_ops.sh:4085-4182`).
6. `safe_remove` is the sink (`lib/core/file_ops.sh:1478-1864`):
   1. Return a pending sticky cancellation (`lib/core/file_ops.sh:1496-1499`).
   2. Run `validate_path_for_deletion`. A refusal logs `SKIPPED <reason>`. A timeout or signal records cancellation (`lib/core/file_ops.sh:1516-1529`).
   3. Check the whitelist again (`lib/core/file_ops.sh:1538-1541`).
   4. Treat a missing path as success (`lib/core/file_ops.sh:1544-1546`).
   5. Refuse a compiled-model cache (`lib/core/file_ops.sh:1551-1555`).
   6. In dry run, record the preview and return (`lib/core/file_ops.sh:1558-1616`).
   7. Size for the log under the section deadline (`lib/core/file_ops.sh:1620-1651`).
   8. Recheck the compiled-model cache, the live owner, SQLite handles and partial downloads after sizing, binding each probe to the parent and target identity (`lib/core/file_ops.sh:1653-1739`).
   9. Compare the caller's expected identity, run the caller's final guard hook, then compare identities again (`lib/core/file_ops.sh:1741-1794`).
   10. Check content hash and expected-absent path (`lib/core/file_ops.sh:1811-1813`, `lib/core/file_ops.sh:2328-2356`).
   11. Run `rm -rf` under `run_with_timeout` with stdin closed (`lib/core/file_ops.sh:1815-1819`).
   12. Classify the result as `REMOVED`, `FAILED removal timed out`, `SKIPPED section time limit reached`, cancellation, `FAILED permission denied` or `FAILED error` (`lib/core/file_ops.sh:1822-1863`).
7. Accounting goes through `mole_add_cleaned_row` and `mole_item_size_continues` (`lib/core/file_ops.sh:4183-4211`).

The privileged variants follow the same shape. `safe_sudo_remove` (`lib/core/file_ops.sh:2009-2295`) refuses symlinks, downgrades below a mutable ancestor, and classifies `auth required`, `sip/mdm protected`, `readonly filesystem` and `sudo error` (`lib/core/file_ops.sh:2271-2288`). `safe_sudo_find_delete` (`lib/core/file_ops.sh:3607-4062`) applies the same per-file gates, captures `dev:ino:mtime` per file, and has a privileged worker re-check age and identity before each unlink (`lib/core/file_ops.sh:3924-4019`).

### 3.2 `mo uninstall`

1. Default mode is Trash. `--permanent` opts out (`bin/uninstall.sh:2097-2118`).
2. Leftover discovery uses `find_app_files` (`lib/core/app_protection.sh:1334`), `find_app_system_files` (`lib/core/app_protection.sh:2288`), and receipts gated by `receipt_payload_path_is_allowlisted` (`lib/core/app_protection.sh:2680`).
3. Preview and confirm require a TTY (`lib/uninstall/batch.sh:2417`, `lib/uninstall/batch.sh:2516-2544`).
4. `MOLE_UNINSTALL_MODE=1` and one `ensure_sudo_session` before execution, real runs only (`lib/uninstall/batch.sh:2572-2592`).
5. `_batch_execute_removals` (`lib/uninstall/batch.sh:2608`) calls `remove_file_list` (`lib/uninstall/batch.sh:770`), which first rechecks ownership after the app move. A reappeared app path or a live or unknown sibling keeps the whole family (`lib/uninstall/batch.sh:778-800`).
6. `remove_file_list` validates each path and logs a refusal as `SKIPPED` (`lib/uninstall/batch.sh:828-835`). In Trash mode, a path that needs no sudo, is not a symlink, and is not an app bundle or container root joins one batch with an identity snapshot (`lib/uninstall/batch.sh:939-957`). `_mole_move_to_trash_batch` rechecks each path's identity right before a direct rename into `~/.Trash` and stops at the first timeout or signal (`lib/core/file_ops.sh:3330-3428`). Each moved path gets a `deletions.log` row `ok` with size `unknown` and an operations row `TRASHED (batch)`. A path whose identity changed or whose move failed stays in place with only a debug line (`lib/uninstall/batch.sh:960-983`).
7. Every other path goes to `mole_delete` (`lib/uninstall/batch.sh:868`, `lib/uninstall/batch.sh:932`, `lib/uninstall/batch.sh:985-996`). `mole_delete` (`lib/core/file_ops.sh:2401-2690`) validates, refuses a privileged delete below a mutable ancestor, sizes (unknown is `unknown`, never 0), checks `dev:ino:mtime` identity, content hash and app absence, then routes to Trash (`lib/core/file_ops.sh:2592-2645`) or to `safe_remove`, `safe_sudo_remove` or `safe_remove_symlink` (`lib/core/file_ops.sh:2647-2689`). Most outcomes write a `deletions.log` row. It returns without a row for an empty path (`lib/core/file_ops.sh:2409`), a path that no longer exists (`lib/core/file_ops.sh:2425-2427`), a hash given without an identity (`lib/core/file_ops.sh:2530`), and a UUID-container ownership timeout or signal (`lib/core/file_ops.sh:2560`).
8. `_mole_move_to_trash` (`lib/core/file_ops.sh:2844-2930`) tries, in order: a privileged staged move, a direct rename for app bundles and container roots with Finder as the fallback for a denied app, the `trash` CLI, then Finder AppleScript with the path passed as argv. Each attempt rechecks the owner and identity first (`lib/core/file_ops.sh:2744-2776`).

### 3.3 `mo purge`

Project discovery refuses artifact names as containers (`lib/clean/project.sh:40`), collapses nested artifacts (`lib/clean/project.sh:1457`), checks open files on every artifact type (`lib/clean/project.sh:778`), marks anything modified within `MIN_AGE_DAYS=7` as recent and unselected (`lib/clean/project.sh:17`, `lib/clean/project.sh:1690`), and ends at `safe_remove` with a purge-specific final guard (`lib/clean/project.sh:1716-1720`, `lib/clean/project.sh:3689`).

### 3.4 `mo analyze`

`moveToTrash` validates the raw path, resolves it with `filepath.Abs`, validates again, then tries `/usr/bin/trash`, an exclusive `renameatx_np(RENAME_EXCL)` into the per-volume Trash, then Finder, each under a 30-second timeout (`cmd/analyze/delete.go:138-162`, `cmd/analyze/delete.go:189-217`, `docs/SECURITY_DESIGN.md:205-211`). The Trash directory must be a real directory owned by the user and not group or world writable (`cmd/analyze/delete.go:261-284`).

### 3.5 Shape for Rust types

A suggested typestate that keeps each stage's evidence attached to the candidate:

```rust
struct Discovered { path: AbsPath }                       // shape-checked, normalized
struct Filtered   { path: AbsPath, policy: PolicyVerdict } // not protected, not whitelisted
struct Sized      { path: AbsPath, size: Size }            // Size::Known(u64) | Unknown | Partial(u64)
struct Approved   { path: AbsPath, identity: PathIdentity, owner: OwnerState::Idle }
enum Outcome { Removed(Size), Trashed(Size), Skipped(Reason), Failed(Reason), Cancelled(Signal) }
```

`PathIdentity` is the physical parent path plus parent and target `(dev, ino)` (`lib/core/file_ops.sh:3282-3318`). Mole's checks are path-based and do not hold a lease across the final syscall (`SECURITY_AUDIT.md:434`, M02). Rust can narrow that gap by opening the parent with `O_NOFOLLOW | O_DIRECTORY`, comparing `fstat` against `PathIdentity`, and calling `unlinkat` relative to that descriptor. That is a design inference, not current Mole behaviour.

## 4. Dry-run parity

### 4.1 What must match between dry run and real run

- The eligible set. Discovery, cheap filters, protection, whitelist, compiled-model, owner and open-handle probes run before the dry-run branch (`lib/core/file_ops.sh:1548-1557`, `lib/core/file_ops.sh:2049-2055`, `bin/clean.sh:238-299`). Two shipped defects broke this (`bugs/deletion-evidence-and-final-sink.md:53-58`).
- Post-sizing rechecks. When the dry run measured after validation, the recorder re-runs protection, whitelist, compiled-model, live-cache and SQLite guards (`lib/core/file_ops.sh:1438-1445`, `bin/clean.sh:244-279`).
- Refusal rows. A refusal logs `SKIPPED <reason>` in both modes because validation precedes the dry-run branch (`lib/core/file_ops.sh:1527`, `lib/core/file_ops.sh:1539`).
- The cancellation contract. A preview ledger is downstream work and stops on a sticky cancel (`bugs/shell-and-test-pitfalls.md:89`).
- Sizing method, unknown sizes, and the `+` partial marker (`bugs/state-accounting-and-progress.md:29-36`).
- Deduplication by filesystem identity, so aliases and symlinks appear once (`SECURITY_AUDIT.md:322`, `bin/clean.sh:287-301`).
- The preview total equals what a real run would free. A real run frees an overlapping parent and child once, so the preview counts the child under its ancestor (`bin/clean.sh:287-300`).
- Process-state answers. A dry run refuses on running and unknown owners exactly like a real run (`bin/clean.sh:255-260`).

### 4.2 Intentional differences

- Nothing is mutated, killed or unloaded. `force_kill_app` returns early (`lib/core/app_protection.sh:2752-2755`). Batch uninstall runs `launchctl` teardown only outside dry run (`SECURITY_AUDIT.md:305`).
- The recursive container open-handle probe moves after sizing (`lib/core/file_ops.sh:1501-1515`).
- `mo clean --dry-run` only adopts an already cached sudo session and never prompts, so the system section may be absent (`bin/clean.sh:1611-1625`).
- Uninstall dry run skips the container system-entry walk and the reappeared-app check (`lib/uninstall/batch.sh:750-762`, `lib/uninstall/batch.sh:791`).
- macOS writes a session marker named `<command>-dry-run`, `SKIPPED` rows, `deletions.log` rows with status `dry-run` (`lib/core/log.sh:267-281`, `lib/core/file_ops.sh:2581`), and the preview list `~/.config/mole/clean-list.txt` (`bin/clean.sh:49`).
- Windows dry runs write no operation rows (`win:lib/core/log.ps1:33-35`, `AGENTS.md:133`) but may create local report files (`win:SECURITY_AUDIT.md:76`).
- Mole's own temp-root housekeeping runs in dry runs and writes no log row (`.claude/rules/core.md:28`).

## 5. Privilege model

### 5.1 Who runs what

- The process always starts as the invoking user. `mole`, `bin/clean.sh` and `bin/touchid.sh` exit 1 when `EUID` is 0 (`mole:8-13`, `bin/clean.sh:8-13`, `bin/touchid.sh:11`).
- User-level work runs with the user's own permissions: user caches, logs, Trash moves, Analyze, purge.
- Privileged work runs through `sudo -n` under a deadline (`_mole_bounded_sudo`, `lib/core/file_ops.sh:56-82`). Callers are `safe_sudo_remove`, `safe_sudo_find_delete`, the privileged Trash stage, orphaned system services (`SECURITY_AUDIT.md:126`), root-owned plist reads, and the optional `sudo -n lsof` used only when a session already exists (`lib/core/file_ops.sh:742-764`).

### 5.2 How sudo is acquired

- `mo clean` asks once before any section. Enter proceeds to authentication. Typed characters are kept and passed to sudo as the start of the password. Space skips. Esc quits. Two other keys skip (`bin/clean.sh:550-613`). Typed password characters must never be read as skip (`AGENTS.md:116`).
- A skip sets `SYSTEM_CLEAN=false` for every later section of that run. A cached credential is not renewed consent. The choice is not persisted (`AGENTS.md:116`).
- Non-interactive runs and dry runs only adopt an existing session (`bin/clean.sh:1611-1640`, `lib/core/sudo.sh:311-347`).
- Uninstall asks once, before execution, only when a selected app needs admin or is a Homebrew cask, and never in dry run (`lib/uninstall/batch.sh:2576-2592`).
- `request_sudo_access` (`lib/core/sudo.sh:120-218`) decides in this order. Test mode refuses. An existing `sudo -n true` session is reused. Without a TTY it shows an `osascript` dialog with the message escaped for AppleScript. In clamshell mode it asks for the password. With Touch ID configured, one `sudo -v` owns the whole exchange, because killing it and starting another opened the dialog twice and discarded a typed password (I-28).
- A keepalive refreshes with `sudo -n -v` every 30 seconds and retries every 5 seconds after a failure, exiting with the parent (`lib/core/sudo.sh:255-291`).

### 5.3 Touch ID

- Touch ID counts as enabled only for an uncommented `pam_tid.so` line in `/etc/pam.d/sudo_local` or `/etc/pam.d/sudo` (`lib/core/sudo.sh:24-49`, I-08).
- `mo touchid` writes PAM files with `sudo install -m 444 -o root -g wheel`, so no user-writable window exists (`bin/touchid.sh:36`, `SECURITY_AUDIT.md:283`).
- The PAM path overrides are honored only in test mode (`lib/core/sudo.sh:12-22`, `AGENTS.md:115`).

### 5.4 Refused outright

- Running as root (`mole:8-13`).
- Deleting a critical root or protected path, with or without sudo (`SECURITY_AUDIT.md:128-140`, `lib/core/file_ops.sh:2024-2032`).
- Sudo-deleting a symlink (`lib/core/file_ops.sh:2038-2041`).
- Any privileged delete or move through an ancestor the invoking user can change (`lib/core/file_ops.sh:1954-2006`, `lib/core/file_ops.sh:2057-2067`, `lib/core/file_ops.sh:2449-2458`).
- Retrying a SIP or MDM refusal. It is logged `FAILED sip/mdm protected` (`lib/core/file_ops.sh:2275`).
- Any `sudo`, `osascript` or `launchctl` call in test mode (`lib/core/file_ops.sh:2079-2086`, `lib/core/file_ops.sh:2585-2590`, `AGENTS.md:115`).
- Software Update staging (`/Library/Updates`, `/macOS Install Data`) and `com.apple.*` launchd jobs (`AGENTS.md:119`, `SECURITY_AUDIT.md:126`).

The sink error codes are `MOLE_ERR_SIP_PROTECTED=10`, `AUTH_FAILED=11`, `READONLY_FS=12`, `PROTECTED_PATH=13`, `PRIVACY_DENIED=14`, `MUTABLE_PARENT=15`, `OWNER_UNVERIFIED=16`, `APP_REAPPEARED=17` (`lib/core/file_ops.sh:13-21`).

### 5.5 Windows

Commands run with the caller's token. Admin-only maintenance checks the token with `Test-IsAdmin`, and elevation never bypasses protected paths (`win:lib/core/base.ps1:258-266`, `win:SECURITY_AUDIT.md:72`).

## 6. Operation log and history

### 6.1 `operations.log`

- The log lives at `~/Library/Logs/mole/operations.log` (`lib/core/log.sh:26`). `MO_NO_OPLOG=1` disables it (`lib/core/log.sh:254-258`). It rotates once per session to `operations.log.old` above 5 MB (`lib/core/log.sh:28`, `lib/core/log.sh:112-121`).
- Each row has the form `[YYYY-MM-DD HH:MM:SS] [<command>[-dry-run] run=<id>] <ACTION> <path> (<detail>)` (`lib/core/log.sh:131-133`, `lib/core/log.sh:273-309`). Outside a session the bracket holds only `<command>`. The run id is `<timestamp digits>-<pid>-<random>-<random>` (`lib/core/log.sh:322`).
- A session starts with an empty line and `# ========== <cmd> run=<id> session started at <ts> ==========`, and ends with `# ========== <cmd> run=<id> session ended at <ts>, <items> items, <size> ==========` (`lib/core/log.sh:313-359`).
- The actions are `REMOVED`, `TRASHED`, `SKIPPED`, `FAILED`, `REBUILT` (`lib/core/log.sh:293`, `lib/core/file_ops.sh:2602`, `lib/core/history.sh:27-31`). The details seen in sinks include a human size, `whitelist`, `protected`, `live user cache`, `active or unknown SQLite database`, `identity changed`, `compiled model cache`, `sink guard denied`, `removal timed out`, `section time limit reached`, `permission denied`, `auth required`, `sip/mdm protected`, `readonly filesystem`, `privacy permission denied`, `mutable-parent`, `trash-failed`.
- Control bytes are written as `\n`, `\r`, `\t` or `\xHH`. Backslashes stay literal (`lib/core/base.sh:805-830`, `lib/core/log.sh:61-94`). The escape applies to single and batched appends.
- A failed append warns once per session and never aborts the operation (`lib/core/log.sh:50-59`).
- Appends are synchronous (`.claude/rules/core.md:25`).
- Mole Mac and the support diagnostic parse this format, so it is a cross-product contract (`AGENTS.md:133`). Analyze writes the same grammar as `[analyze run=<id>]` (`cmd/analyze/oplog.go:16-37`).

### 6.2 `deletions.log`

- The log lives at `~/Library/Logs/mole/deletions.log` and can be overridden with `MOLE_DELETE_LOG` (`lib/core/file_ops.sh:3436`).
- `mole_delete` writes it on most outcomes (section 3.2 lists the exceptions). The uninstall Trash batch writes one `ok` row per moved path with size `unknown` (`lib/uninstall/batch.sh:972`). A failed batch move writes no row.
- Each row has the form `<%Y-%m-%dT%H:%M:%S%z>\t<mode>\t<size_kb or unknown>\t<status>\t<path>` (`lib/core/file_ops.sh:2394-2400`, `lib/core/file_ops.sh:3450-3459`). Every field is escaped.
- The statuses are `ok`, `rejected`, `dry-run`, `timed-out`, `interrupted`, `identity-changed`, `mutable-parent`, `privacy-denied`, `trash-failed`, `invalid-mode`, `sudo-blocked-test-mode`, `ownership-unverified`, `app-reappeared`, `error` (`lib/core/file_ops.sh:2401-2690`).

### 6.3 Other logs and `mo history`

- `mole.log` rotates at 1 MB. `mole_debug_session.log` holds `--debug` output (`lib/core/log.sh:24-28`, `lib/core/log.sh:97-110`).
- `mo history` dispatches before the rest of the CLI loads and is read-only (`mole:94-107`, `SECURITY_AUDIT.md:329`). It reads both logs (`lib/core/history.sh:62-67`), escapes legacy control bytes in text output, and in `--json` replaces each malformed UTF-8 byte with U+FFFD without rewriting the logs (`lib/core/history.sh:689`, `SECURITY_AUDIT.md:46`).
- The logs are user-writable and are not tamper evidence (`SECURITY_AUDIT.md:444`, M12).
- Windows writes the same row format to `%USERPROFILE%\.config\mole\operations.log`, rotated at 5 MB (`win:lib/core/log.ps1:33-36`).

## 7. Top 10 incidents a new implementation is likely to repeat

1. A folder named like a cache held installed binaries, models or dependencies (Camoufox I-42, Cypress I-45, Puppeteer I-48, Buzz I-63, npx I-11, Yarn Berry I-22). Catalog rows `docs/incidents.md:48`, `docs/incidents.md:59`, `docs/incidents.md:79`, `docs/incidents.md:82`, `docs/incidents.md:85`, `docs/incidents.md:100`.
2. Caches were cleaned while the owning app ran, or while the probe could not tell (Autodesk Fusion disk-fill #1390, Chrome I-20, Adobe I-21 and I-26, Electron I-39). Cites `SECURITY_AUDIT.md:123`, `docs/incidents.md:57`, `docs/incidents.md:76`, `bugs/deletion-evidence-and-final-sink.md:25-45`.
3. A display name or id prefix claimed another app's data (Claude.app took Claude Code CLI state, `com.foo` matched `com.foobar`, a stable app took a Nightly build's folders I-24). Cites `bugs/deletion-evidence-and-final-sink.md:5-15`, `docs/incidents.md:61`.
4. A generic parent sweep removed a child that a stricter row guarded (`~/Library/Caches/Google` I-37, moving `Library/Containers` took Docker and OrbStack state). Cites `docs/incidents.md:74`, `SECURITY_AUDIT.md:34`.
5. Directory mtime was read as content age, so a folder still being written looked idle (I-46, I-47). Cites `docs/incidents.md:83-84`.
6. Dry run and real run computed different eligible sets (protection checked only in real mode, an active-process guard checked only in real mode, double-recorded dry-run targets I-10). Cites `bugs/deletion-evidence-and-final-sink.md:53-58`, `docs/incidents.md:47`.
7. A failed or partial read was treated as absence (simulator metadata I-16, unreadable live registry I-17, unreadable or empty config replaced by discovery I-43, I-44, I-49, a TTL cache accepted as proof that no sibling existed). Cites `docs/incidents.md:53-54`, `docs/incidents.md:80-81`, `bugs/state-accounting-and-progress.md:19-21`.
8. The container itself was deleted instead of its contents, or a structural folder was offered (DiagnosticReports directory I-03, iCloud stores I-27, sandbox `Data/Downloads` alias #1578). Cites `docs/incidents.md:40`, `docs/incidents.md:64`, `bugs/deletion-evidence-and-final-sink.md:103-121`.
9. A timeout or exit status was classified wrong, so one slow scan cancelled the run or an interrupted probe was treated as a skip (I-61, I-62, #1695). Cites `docs/incidents.md:98-99`, `bugs/shell-and-test-pitfalls.md:64-97`.
10. A test-only environment seam redirected what a real run deleted, two of them through sudo (I-64). Cite `docs/incidents.md:101`.

## Unresolved

- The deletion sinks run `rm -rf` with no device-boundary option (`lib/core/file_ops.sh:1816-1818`, `lib/core/file_ops.sh:2240`). Sizing stops at a device change (`bin/clean.sh:776`), the privileged `chown` uses `-x` (`lib/core/file_ops.sh:3186`), and `--external` scans use `-xdev` (`lib/clean/user.sh:1978`). No rule found stops a recursive delete at a mount point inside the target. Decide this for the Rust sink.
- `safe_sudo_find_delete` (`lib/core/file_ops.sh:3607-4062`) and `_mole_move_path_to_user_trash` (`lib/core/file_ops.sh:2993-3262`) were skimmed by keyword, not read line by line.
- The Linux branch was not read. Every Linux note is inference.
- `docs/SECURITY_DESIGN.md` was last reviewed at V1.59.0 (`docs/SECURITY_DESIGN.md:261`) and understates the validator's pre-allowlist checks.
