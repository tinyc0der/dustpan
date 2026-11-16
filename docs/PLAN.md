# Dustpan plan

Dustpan is a working name for an original Rust disk cleaner with a terminal UI and a scriptable CLI. It covers everything Mole (`tw93/Mole`) does on macOS first, then Windows, then Linux. This plan holds four things. The first is the feature inventory taken from Mole's source. The second is the TUI design. The third is the architecture. The fourth is a milestone sequence in which each milestone ends in something you can run and check.

One rule runs through every milestone. Nothing leaves the disk until the user has seen it on a review screen, or has passed `--yes` to a command whose `--dry-run` shows the same list. Every deletion passes one guard pipeline and writes one log row.

Milestones in order are M0 foundations, M1 analyze, M2 status, M3 clean (user), M4 clean (admin) and optimize, M5 uninstall, M6 purge and installers, M7 slim and lifecycle (macOS 1.0), M8 Windows, and M9 Linux. `docs/M0-checklist.md` breaks M0 into PRs.

## How to read this

The feature inventory and the TUI design say what to build. The architecture says how the code is shaped. The milestones say in what order to build it and what evidence closes each one. The appendices hold the spike questions, the alternatives this plan rejected, the risks, and the reading list.

Each feature row cites an ID from `docs/inventory/`. Those seven files were read from Mole's source, with a `file:line` for every claim. Read the inventory row before you build the feature.

The program runs `skills/poteto-mode/playbooks/autopilot-stack.md` from the installed plugin, because each PR builds on the one before it. The operator lands every PR. Each milestone's last PR is an operator item that stops at merge-ready.

Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

## Decide these before M0 starts

The plan applies a default to each item below. The name needs your call before the first public commit. The other two defaults hold unless you say otherwise.

1. **Name.** "dustpan" is a placeholder for the crate and the binary. Mole's `TRADEMARK.md` rules out "Mole", `mo`, the Mole logo, and any name that implies endorsement. Pick a name you can register on crates.io and Homebrew.
2. **License.** The default is GPL-3.0-or-later, as the conservative choice. Mole's code is GPL-3.0, and dustpan ports Mole's cleanup catalogs, protection lists and incident rules closely. Dustpan copies no Mole code, as `docs/inventory/linux.md` note 15 advises. But a port that close may still count as a derivative work, and this plan budgets for neither a legal review nor a clean-room rewrite of the catalogs. GPL-3.0-or-later holds either way. The Linux leftover paths from xdg-ninja are MIT and need their notice (`linux.md` section 9, note 9).
3. **Hosting.** The default is a new GitHub repository under your account, created in PR M0-1. CI needs macOS, Windows and Linux runners from the first PR.

## Feature inventory

The rows below cover every command and flag the three Mole dispatchers accept. Sources are `main` at `5dd93fe3` (macOS, Mole 1.72.0), `windows` at `59f3dc5a`, and `linux` at `4de579cc`. `scripts/inventory-gate.sh` checks completeness. It extracts every subcommand and long flag from `mole`, `mole.ps1` and the command scripts on all three branches, and it fails on any one the inventory does not name. It also reads the flags the Go TUIs declare. Its last run printed `macos missing: 0`, `windows missing: 0`, `linux missing: 0`. A control run with three real flags removed from a copy of the inventory exited 1 and named each one. The 19 tokens in `scripts/inventory-gate.ignore` belong to tools Mole calls, such as `fd`, `git` and `systemctl`. `scripts/README.md` gives the reason for each. Run against this file alone, the gate reports only flags of tools Mole calls and of Mole's install scripts, which dustpan does not port.

Mole's macOS CLI has 15 commands, about 27 long flags, 34 cleanup categories in `clean`, and 19 optimize tasks. The Windows CLI has 10 commands and 37 declared flags. The Linux CLI has 9 commands and 27 flags, all implemented, with apt and pacman as the only package managers.

### What dustpan must do

"yes" means Mole implements the feature on that branch. "no" means the branch has no code for it. The milestone column names where dustpan builds it for macOS. Windows and Linux land in M8 and M9.

| # | Feature | macOS | Windows | Linux | Inventory | Milestone |
|---|---|---|---|---|---|---|
| F01 | Home menu with five entries, number keys, update notice | yes | yes | yes | CORE-05, MENU-01, WIN-CORE-01 | M0 |
| F02 | `--help`, `--version` with system facts, global `--debug` session log | yes | yes | yes | CORE-02..04, WIN-CORE-02..04 | M0 |
| F03 | Operations log and deletions log in a stable row format | yes | yes, no dry-run rows | yes | safety section 6 | M0 |
| F04 | `analyze` overview of home, Library, Applications, volumes, and "hidden space" insights | yes | yes | yes | ANALYZE-01, 20, 21; WIN-ANA-01 | M1 |
| F05 | `analyze PATH` live drill-down, refresh, filter, top 20 files, stale and cleanable hints | yes | yes | yes | ANALYZE-02..11, 18, 19 | M1 |
| F06 | `analyze` open, reveal, preview, multi-select, move to Trash | yes | Recycle Bin | freedesktop Trash | ANALYZE-12..17, 22; WIN-ANA-05 | M1 |
| F07 | `analyze --json` | yes | yes | yes | ANALYZE-04, WIN-ANA-03 | M1 |
| F08 | `status` dashboard with CPU, memory, disk, power, processes, network, health score | yes | yes, no thermal term | yes | STATUS-01, 07..09; WIN-STAT-01, 04 | M2 |
| F09 | `status --json`, `--watch`, `--interval`, and process CPU alerts with `--proc-cpu-alerts`, `--proc-cpu-threshold`, `--proc-cpu-window` | yes | alert flags not forwarded | yes | STATUS-02..05 | M2 |
| F10 | `clean` user caches, logs, Trash, browsers, cloud and office, developer tools, apps, virtualization | 14 sections, 34 categories | 5 stages | rules table | CLEAN-01, 12..20; WIN-CLEAN-01..06 | M3 |
| F11 | `clean --dry-run` with a preview list file | yes | yes | yes | CLEAN-02, 29; WIN-CLEAN-07 | M3 |
| F12 | `clean --whitelist` protected-paths editor | menu | Notepad | yes | CLEAN-04, WL-01; WIN-CLEAN-11 | M3 |
| F13 | `clean` leftovers of apps already removed, container stubs | yes | no | no; Flatpak orphans are listed by `uninstall` (LNX-UNIN-12) | CLEAN-21, 23 | M3 |
| F14 | `clean` review-only rows for large files, snapshots, project artifacts, LaunchAgents, iOS backups | yes | Docker build cache only | report rows for Flatpak runtimes, Snap revisions, core dumps, containers, large unknown caches | CLEAN-11, 24, 27, 28; LNX-CLEAN-27..30 | M3 |
| F15 | `clean --external VOLUME` for `.DS_Store`, `._*`, `.TemporaryItems` | yes | no | no | CLEAN-03 | M3 |
| F16 | `history`, `--json`, `--limit` | yes | yes | yes | HIST-01..03; WIN-HIST-01, 02 | M0 |
| F17 | `clean` system caches with admin rights | sudo | `--system`, already elevated only | pacman, apt, journald through sudo | CLEAN-08..10, 22, 25, 26; WIN-CLEAN-08 | M4 |
| F18 | `optimize` maintenance tasks with one outcome per task | 19 tasks | 8 steps | 21 checks, mostly report-only | OPT-00..19; WIN-OPT-01..10; LNX-OPT | M4 |
| F19 | `optimize --whitelist` task opt-out | yes | no | no | OPT-WL | M4 |
| F20 | Performance diagnosis for sustained CPU, swap, idle VMs, runaway processes | yes | no | yes, from PSI pressure | OPT-DIAG; LNX-OPT-03 | M4 |
| F21 | `touchid enable`, `disable`, `status`, and the sudo hint | yes | no | no | TOUCHID-01..06 | M4 |
| F22 | `uninstall` picker with search, sort and sizes | yes | yes | yes | UNINST-01, 10; WIN-UNIN-01 | M5 |
| F23 | `uninstall NAME...` and `--list` (JSON when piped) | yes | `--list`, `--rescan` | yes, `--yes` | UNINST-02, 03; WIN-UNIN-02, 03 | M5 |
| F24 | Leftover discovery, personal-data split, `--with-data`, `D` chooser | yes | install folder only | exact ids | UNINST-06, 11..13; WIN-UNIN-07 | M5 |
| F25 | Trash by default, `--permanent` to skip it | yes | Recycle Bin | Trash | UNINST-05 | M5 |
| F26 | Uninstall side effects such as quit, agents, login items, LaunchServices, Dock, Homebrew cask, official-uninstaller block, config hints | yes | vendor uninstaller, MSIX | apt, pacman, snap, flatpak | UNINST-14..22; WIN-UNIN-05, 06 | M5 |
| F27 | `purge` project build artifacts, `--paths`, `--yes`, `--include-empty` | yes | yes | yes | PURGE-01..11; WIN-PURGE-01..05 | M6 |
| F28 | `installer` cleanup of downloaded installers | `.dmg .pkg .mpkg .iso .xip`, installer zips | `.msi` in Downloads, 14 days | no | INST-01..06; WIN-INST-01, 02 | M6 |
| F29 | `slim` removes Intel code from universal apps | yes | no | no | SLIM-01..09 | M7 |
| F30 | `completion` for bash, zsh, fish | partial flag coverage | no | no | COMP-01, 02 | M7 |
| F31 | `update`, `--force`, `--nightly`, background update notice | yes | yes | yes | UPD-01..05, CORE-06 | M7 |
| F32 | `remove` uninstalls the tool itself | yes | yes | yes | RM-01 | M7 |
| F33 | `clean --game-media`, `--gpu-shaders` opt-ins | no | yes | `--gpu-shaders` | WIN-CLEAN-09, 10 | M8 |
| F34 | Windows discovery from the registry and MSIX, Delivery Optimization, Recycle Bin sinks | no | yes | no | `windows.md` sections 4, 7 | M8 |
| F35 | Linux package-manager caches, Flatpak, Snap, journald, freedesktop Trash | no | no | yes | `linux.md` sections 3.4, 7 | M9 |

### Where dustpan differs from Mole on purpose

Each change below makes the tool safer or easier to use. Each one is a decision this plan records, not an omission.

- Every destructive command takes `--dry-run`, `--json` and `--yes`, and every read-only command takes `--json`. Mole has no JSON for `clean`, `purge`, `installer` or `slim`.
- A destructive command with no terminal needs `--yes`. Mole's `clean` runs unattended without one (CLEAN-09).
- Installers and `analyze` deletions go to the Trash by default. Rebuildable caches and build artifacts are deleted for good, because moving them to the Trash frees nothing. Mole deletes installers for good on macOS (porting note 1 in `mac-clean-purge-installer.md`).
- Every destructive flow ends on the same review screen, and the confirm dialog states how many bytes each sink handles.
- Uninstall asks before it quits a running app. Mole kills it after confirmation with only a `[Running]` tag (UNINST-15, porting note 10).
- The delete sink stops at mount points inside the target. Mole's safety inventory leaves this unresolved.
- Exit codes are the same on all three platforms. Mole exits 2 on Windows and 1 elsewhere for an unknown command.
- `clap_complete` generates shell completion from the command tree, so every flag completes on every shell.
- Every screen has a `?` help overlay. Mole's `status` has no key legend.
- Windows builds ship for x64 and ARM64 and handle paths longer than 260 characters.
- `dustpan explain PATH` prints whether the guard would keep or remove a path, and which layer decided. Mole has no such command. Its kept rows show a short reason only during a run.

Dustpan does not port these: optimize tasks Mole retired (`mac-optimize-core-cli.md` section 3.1), the removed `clean --select`, `--categories` and `--exclude` flags, features of the paid Mole for Mac and Mole for Windows apps, `plugins/`, `skills/`, the Nix files, and `install.sh`. Packaging replaces the install script in M7.

## TUI design

### What friendly means here

1. The user always knows what will happen. Every row says how it leaves the disk (Delete, Trash, Run tool, Admin, or Kept) before anything happens.
2. A scan never blocks the screen. Sizes stream in as they are measured, and Esc cancels at any point.
3. A size the tool could not measure reads `unknown` or ends in `+`. It never reads `0 B`.
4. The same keys work on every screen, and `?` lists them.
5. Color is never the only signal. Every state also has a symbol or a word, and the UI works with `NO_COLOR` and in ASCII.

### Every destructive flow ends on the same four screens

```text
Home ─┬─ Clean ──────── scan ─> Review ─> Confirm ─> Progress ─> Summary
      ├─ Uninstall ──── picker ─> Review ─> Confirm ─> Progress ─> Summary
      ├─ Analyze ────── overview <-> directory <-> top files ─> Confirm (Trash)
      ├─ Optimize ───── task list ─> Confirm ─> Progress ─> Summary
      ├─ Status ─────── dashboard
      ├─ Purge ──────── scan ─> Review ─> Confirm ─> Progress ─> Summary
      ├─ Installers ─── scan ─> Review ─> Confirm ─> Progress ─> Summary
      ├─ Slim ───────── scan ─> Review ─> Confirm ─> Progress ─> Summary
      ├─ History ────── sessions ─> session detail
      └─ Settings ───── protected paths, skipped optimize tasks, purge folders,
                        Touch ID, shell completion, updates, appearance
```

Review, Confirm, Progress and Summary are one set of widgets shared by every destructive flow. Each flow only supplies its candidate groups.

### Every screen shares one frame

Every screen has the same four bands. The header shows the breadcrumb, the free space on the volume, and a `⚙ admin` badge while elevated rights are live. The body is a list on the left and a detail pane on the right. The footer shows key hints. A one-line toast area above the footer shows results such as "Moved 3 items to Trash".

```text
 dustpan › Clean › Review                         ⚙ admin            Macintosh HD  88 GB free
─────────────────────────────────────────────────────────────────────────────────────────────
 Selected 9.8 GB of 14.2 GB · 1,204 items                  │ Chrome cache
                                                           │ ~/Library/Caches/Google/Chrome/Default/Cache
 ▾ [■] Developer tools            6.1 GB                   │
     [x] Xcode DerivedData        3.2 GB  Delete           │ Size     1.4 GB on disk
     [x] npm cache                1.1 GB  Run npm          │ Age      untouched 12 days
     [ ] Gradle caches            1.8 GB  Kept: protected  │ Removal  Deleted for good. Chrome
 ▾ [■] Browsers                   2.4 GB                   │          rebuilds it on next launch.
 ➤   [x] Chrome cache             1.4 GB  Delete           │ Check    Chrome is not running ✓
     [-] Safari cache             1.0 GB  Safari running   │ Inside   3 protected paths are kept
 ▸ [ ] System caches              1.7 GB  Needs admin      │
─────────────────────────────────────────────────────────────────────────────────────────────
 Space Toggle  a All  ←→ Fold  / Filter  p Protect  Enter Clean 9.8 GB  Esc Back  ? Help
```

At 100 columns or more the detail pane sits beside the list. Between 60 and 99 columns the list takes the full width and `Tab` swaps to the detail pane. Below 60 by 16 the screen shows the minimum size and waits for a resize. Footer hints drop by priority as the width shrinks, which is how Mole's paginated menu behaves (UI-01).

### Confirm states every sink

```text
 ┌ Clean 9.8 GB? ─────────────────────────────────────────┐
 │ 1,204 items in 7 groups                                 │
 │   8.7 GB  deleted for good (caches apps rebuild)        │
 │   1.1 GB  removed by npm's own cache command            │
 │   0 B     moved to Trash                                │
 │ Not touched: Safari cache (Safari is running)           │
 │                                                         │
 │          Enter  Clean            Esc  Go back           │
 └─────────────────────────────────────────────────────────┘
```

Only a real Enter key event confirms. End of input, a closed terminal, or keys typed during a scan never count as Enter. Mole learned this in #726 and drains input before every prompt. Dustpan does the same.

### The same keys work on every screen

| Key | Action | Where |
|---|---|---|
| `↑` `↓` or `k` `j` | Move | Everywhere |
| `Enter` or `→` or `l` | Open, or run the primary action | Everywhere |
| `Esc` or `←` or `h` | Back; clears an active filter first | Everywhere |
| `Space` | Toggle the row | Lists with selection |
| `a` | Select every eligible row in view | Review screens |
| `/` | Filter by words, all must match | Lists |
| `s` / `o` | Cycle the sort key / reverse the order | Lists |
| `Tab` | Swap list and detail pane | Narrow terminals |
| `p` | Protect the path (adds it to the whitelist) | Clean review, Analyze |
| `?` | Help overlay for this screen | Everywhere |
| `1`..`9` | Jump to a home entry | Home |
| `,` | Settings | Home |
| `q` | Quit from Home; back elsewhere | Everywhere |
| `Ctrl+C` | Quit now, after restoring the terminal | Everywhere |

The mouse wheel scrolls, and a click selects a row. Mole drains mouse events (porting note in `mac-uninstall-slim.md` section 4.1). Dustpan reads them, so a wheel event never turns into stray key input.

### What each screen adds

**Home.** It lists Clean, Uninstall, Analyze, Optimize and Status, with Purge, Installers, Slim and History below a divider. Each entry shows one live fact, such as the reclaimable size from the last clean, the app count, the health score, or the date of the last session. A fact appears only when it is cached or cheap. Home never starts a scan by itself.

**Clean.** Category groups appear at once and fill with sizes as each scan finishes. Each scan has a deadline, and a group that times out reads "partial" or "skipped (time limit)". Review opens with each rule's default selection. Admin groups are folded until the user grants rights, and granting suspends the TUI to show the system's own sudo or Touch ID prompt.

**Uninstall.** The picker lists apps with size, last used date and source (App Store, Homebrew, direct). The review splits each app's files into "Removed with the app" and "Your data (kept)". `D` opens the data chooser. A running app gets a "Quit app first" row, and the confirm dialog says the app will be asked to quit.

**Analyze.** It keeps Mole's model of an overview, live drill-down, a top-files view, eighth-cell bars, and the 🧹 and stale hints. It adds a breadcrumb and mouse support. Deleting goes through the shared Confirm dialog and always moves items to the Trash.

**Status.** It uses Mole's six cards in one or two columns depending on width. It adds a footer legend and `?` help. It falls back to a single column, then fewer core rows, before it shows "terminal too small".

**Optimize.** It lists every task with its last outcome (applied, unchanged, skipped, unavailable, attention, failed) and whether it needs admin. Space excludes a task for this run. `p` excludes it for every run, which replaces `optimize --whitelist`.

**History.** It lists sessions newest first with items, size and failures. Enter opens the rows of a session. Rows sent to the Trash show "Reveal in Trash".

**Settings.** It is one screen for everything Mole spreads across `--whitelist`, `--paths`, `touchid` and `completion`.

### The terminal is always restored

Dustpan uses `ratatui` with the `crossterm` backend on the alternate screen, and it restores the terminal on panic, signal and exit. It measures text in display cells with `unicode-width` and cuts long names in the middle, as Mole's analyzer does. It uses ASCII glyphs when the locale is not UTF-8 or `--ascii` is set. Windows needs virtual terminal processing and UTF-8 output, which spike S4 checks. With no TTY, interactive commands print the plan as text or JSON and exit. They never draw a TUI.

## Architecture

### Three crates keep the core free of OS calls

| Crate | Owns | Depends on |
|---|---|---|
| `dustpan-core` | Domain types, the guard pipeline, the catalog loader, the planner and executor, log rows, the JSON schema. No OS calls. | none |
| `dustpan-sys` | One module per OS behind traits. It covers file identity, allocated size, Trash, process probes, privilege, volumes, app inventory, metrics and package managers. | `dustpan-core` |
| `dustpan` | The binary. It holds the `clap` CLI, the `ratatui` TUI, output formats, and the privileged helper entry point. | both |

Catalog data lives in `catalog/<os>/*.toml` and is embedded at build time. Three crates keep the domain pure and testable while the OS code stays thin (**boundary-discipline**). M0-3 runs the **architect** skill on these signatures before any code depends on them.

### Types make an unguarded delete impossible

```rust
pub struct AbsPath(PathBuf);                 // absolute, normalized, no `..`, no control bytes
pub struct FileId { volume: u64, file: u64 } // dev+ino on Unix, volume serial+file index on Windows
pub struct PathIdentity { parent: FileId, target: FileId, mtime: SystemTime }

pub enum Size { Known(u64), Partial(u64), Unknown }      // Unknown never renders as 0
pub enum OwnerState { Idle, Running(Vec<Pid>), Unknown }  // Unknown is treated as Running

pub enum Sink {
    Delete,                       // rebuildable data only
    Trash,                        // anything a person made or might want back
    OwnerCommand(OwnerCommandId), // `npm cache clean`, `brew cleanup`, `docker builder prune`
    Privileged(PrivilegedAction), // closed enum, runs in the elevated helper
    ReviewOnly,                   // shown, never touched
}

pub struct Candidate {
    pub rule: RuleId,
    pub group: GroupId,
    pub path: AbsPath,
    pub identity: PathIdentity,
    pub size: Size,
    pub last_used: Option<SystemTime>,
    pub sink: Sink,
    pub default_selected: bool,
    pub verdict: Verdict,         // Eligible | Kept(KeptReason)
    pub why: Explanation,         // the sentence the detail pane shows
}

pub struct Approved(Candidate);  // only guard::recheck() builds one
pub enum Outcome { Removed(Size), Trashed(Size), Ran(Size), Skipped(KeptReason), Failed(FailReason), Cancelled }
```

These types encode Mole's deletion lifecycle. That lifecycle runs discover, cheap filters, owner probe, sizing, owner re-probe, identity rebind, sink, accounting (`safety-invariants.md` section 3). A sink accepts only `Approved`, so no code path can delete a path that skipped the guard (**type-system-discipline**). Dry run and real run share every step except the sink call. The executor takes `Mode::{Preview, Apply}`, and both return the same `Vec<(Candidate, Outcome)>` shape. Mole shipped two defects where the two modes disagreed (`safety-invariants.md` section 4.1).

### Every path passes five guard layers

A path must pass five layers in order. Mole uses the same layers (`safety-invariants.md` section 2).

1. Path shape. The path is absolute, has no `..` and no control bytes, and has no symlinked ancestor.
2. Critical roots. It is not `/`, a system tree, a home head folder, or a case or symlink alias of one. The check compares file identity, not strings.
3. Live state. The owner app is not running or unknown, no handle is open, and no SQLite WAL or SHM file is in use.
4. Protection policy. The path matches no protected bundle id, path, or compiled-model cache.
5. Whitelist. The user's protected paths, plus hard-safety patterns that a user cannot remove.

`guard::recheck` runs all five layers again at the sink, because SAFE-11 requires the whitelist check inside every sink, not only in callers. It checks against the `PathIdentity` taken at discovery. On Unix the sink opens the parent with `O_NOFOLLOW | O_DIRECTORY`, compares `fstat` with the identity, and calls `unlinkat` relative to that descriptor. This narrows the race window that Mole's audit lists as M02. The recursive delete never crosses a device boundary.

`safety-invariants.md` section 1 lists 39 rules, SAFE-01 to SAFE-39. Each rule has the incident behind it and a test a Rust suite can run. Each rule becomes at least one test in the first milestone whose code it governs. A milestone does not close while one of its rules lacks a test that fails without the guard.

| Rules | First milestone |
|---|---|
| SAFE-01 to SAFE-07, SAFE-10 to SAFE-13, SAFE-16 to SAFE-22, SAFE-27, SAFE-35, SAFE-37 to SAFE-39 | M0 |
| SAFE-34, SAFE-36 | M1 |
| SAFE-08, SAFE-09, SAFE-23, SAFE-25, SAFE-26, SAFE-28 to SAFE-32 | M3 |
| SAFE-14, SAFE-15 | M4 |
| SAFE-24, SAFE-33 | M5 |

Each rule sits where its body, not its title, first applies. SAFE-19 and SAFE-20 (bounded subprocesses, timeout and signal contracts) start in M0 because `--version`, the parity harness and the TUI's signal handling need them. SAFE-27 (protect ancestors of protected state) starts in M0 because the sink deletes whole folders. SAFE-37 starts in M0 because it requires that `history` never writes. SAFE-34 (a real terminal for confirmation) starts in M1 with analyze's Trash confirm. SAFE-23 (exact ownership evidence) starts in M3 because clean's leftover rows claim app data.

SAFE-01 says every removal goes through one sink. M0-1 enforces it with a `clippy.toml` `disallowed-methods` entry for `std::fs::remove_file`, `remove_dir`, `remove_dir_all` and `rename`, allowed only inside the sink module. A stray delete then fails the build instead of waiting for review (**encode-lessons-in-structure**).

### Cleanup targets are data rows

Every cleanup target is one TOML row. Clean, purge, the analyze hints, and the uninstall data split all read the same rows, so a fix to a rule reaches every command.

```toml
[[rule]]
id = "browser.chrome.cache"
group = "browsers"
label = "Chrome cache"
roots = ["~/Library/Caches/Google/Chrome/*/Cache", "~/Library/Caches/Google/Chrome/*/Code Cache"]
owners = [{ process = "Google Chrome" }]
min_age = "0d"
sink = "delete"
protect_label = "Chrome cache"     # row in the protected-paths editor
incidents = ["I-20"]               # Mole incidents this row's tests replay
```

Mole's Linux branch already uses this shape with `section|label|root|owners|selector` (`linux.md` section 9, note 1). A data file plus one regression test per incident replaces most of the 6,335 lines of per-app Bash in `lib/clean/dev.sh` (**model-the-domain**).

### Admin work runs in a separate helper

Dustpan never runs as root. Mole refuses root too (`mole:10-13`). Admin work runs in a short-lived helper, which is the same binary started as `dustpan --privileged-helper`. The parent sends it a list of `PrivilegedAction` values with their identities over a pipe, and the helper runs the guard again as root. On macOS and Linux the parent starts the helper through `sudo`, after it suspends the TUI so the system's password or Touch ID prompt shows. On Windows the helper starts through `ShellExecuteEx` with `runas`. Spike S3 tests this design before M4 depends on it.

### The CLI contract is the same on every OS

The binary has 14 commands, `clean`, `uninstall`, `optimize`, `analyze`, `status`, `purge`, `installer`, `slim`, `touchid`, `history`, `explain`, `completion`, `update` and `remove`, with `analyse` and `optimise` as aliases. `--debug`, `--ascii`, `--no-color`, `--help` and `--version` work everywhere. `--json` works on every command, and `--dry-run` and `--yes` work on every destructive one. A flag a command cannot honor is rejected with exit 2.

| Exit code | Meaning |
|---|---|
| 0 | Done, or cancelled by the user |
| 1 | Finished with at least one failed item |
| 2 | Usage error |
| 3 | Refused, for example a destructive command with no TTY and no `--yes` |
| 124 | A deadline expired |
| 130 | Interrupted by a signal |

Every `--json` output is one object with `schema_version`, `command`, `run_id`, `mode` and `items[]`. Each item carries `path`, `size`, `sink`, `outcome` and `reason`. `history --json` keeps Mole's field names so existing scripts keep working (`mac-optimize-core-cli.md` porting note 9). `operations.log` keeps Mole's row grammar `[ts] [cmd run=id] ACTION path (detail)` in dustpan's own log folder.

### M0 builds the verification tools

- A fixture home. Each test builds a fake home in a temp directory with caches, apps, projects and symlink traps, and sets `HOME`. Mole's Bats suite works the same way.
- A Mole parity harness. It runs `mo <cmd> --dry-run` and `dustpan <cmd> --dry-run --json` against the same fixture home and diffs the candidate paths. Each catalog PR proves itself against Mole this way (**build-the-lever**).
- Golden output. `insta` snapshots of CLI text and of `ratatui` `TestBackend` frames at 60, 80 and 120 columns.
- A PTY driver. A project verification skill made with `/create-verification-skill` in M0-6 drives the real binary in a pseudo-terminal, sends keys, and saves screen captures. Live lanes use it.

## Milestones

Read-only features and the safety core come before any command that deletes (**sequence-verifiable-units**). The platform boundary exists from M0, and CI builds and tests all three OSes from the first PR, so the Windows and Linux ports add modules without reshaping the core (**foundational-thinking**).

| Milestone | Delivers | Platforms | Closes when |
|---|---|---|---|
| M0 Foundations | Workspace, CI, spikes, core types, guard pipeline, sinks, logs, config, TUI frame, CLI skeleton, parity harness | macOS, with Windows and Linux compiling | Every PR in `docs/M0-checklist.md` is merged with its evidence |
| M1 Analyze | F04..F07 | macOS | `analyze --json ~` matches `mo analyze --json ~` per top-level entry within 1%, and the PTY scenario passes |
| M2 Status | F08, F09 | macOS | `status --json` has every field Mole's has, and an idle dashboard uses no more CPU than `status-go` |
| M3 Clean (user) | F10..F15 | macOS | The parity harness shows no candidate in dustpan's set that Mole's set lacks, and every gap is listed with a reason |
| M4 Clean (admin) and optimize | F17..F21 | macOS | The helper runs the admin catalog in a VM snapshot with no protected-path hit; all 19 optimize tasks report one outcome each |
| M5 Uninstall | F22..F26 | macOS | 20 fixture apps uninstall with leftovers matched; same-bundle-id sibling fixtures keep everything |
| M6 Purge and installers | F27, F28 | macOS | Purge parity on the fixture project tree; installer zip detection matches Mole on the fixture set |
| M7 Slim and lifecycle | F29..F32; Homebrew tap and release builds | macOS | macOS 1.0 ships; `slim` verifies codesign on a copy before any swap |
| M8 Windows | F33, F34, plus every earlier feature on Windows | Windows | Same gates as M1..M7 on Windows 11 x64 and ARM64; paths over 260 characters pass |
| M9 Linux | F35, plus every earlier feature on Linux | Linux | Same gates on Ubuntu LTS and Arch; dnf and zypper listed as follow-up |

### M0 Foundations

Spikes come first, because three of them can change the core types. Then come the safety core and the scaffolding every later milestone uses. `docs/M0-checklist.md` lists seven PRs with their evidence. M0 ships no cleaning, but `dustpan` opens to a working Home screen and `--help`, `--version`, `explain` and `history` work.

- [ ] S1 to S5 spike results recorded in Appendix A, each with a verdict.
- [ ] `dustpan-core` guard pipeline refuses every critical-root, symlink-ancestor and alias fixture from `safety-invariants.md` sections 2.1 to 2.4.
- [ ] `Delete` and `Trash` sinks pass the fixture suite on macOS and compile on Windows and Linux.
- [ ] The parity harness runs `mo clean --dry-run` against a fixture home and parses its `clean-list.txt`.

### M1 Analyze

Analyze comes first among features because it deletes nothing except through the Trash, and it exercises the scanner, sizing semantics, cache, list widgets and the Trash sink.

- [ ] Overview rows, live drill-down, top files, filter, refresh and multi-select work as in ANALYZE-01..17.
- [ ] Sizes are allocated bytes, hardlinks count once, APFS clones that share all blocks count once, and scans stop at mount points (`mac-analyze-status-tui.md` section 2.2).
- [ ] The analyze protection list refuses every path in `mac-analyze-status-tui.md` section 2.8, step 5.
- [ ] The shared Confirm dialog, which every later destructive flow reuses. Only a real Enter key event confirms (SAFE-34).
- [ ] Perf. Measure `mo analyze --json ~` first, then `dustpan analyze --json ~`, interleaved, five runs each, cold cache. Fail if dustpan's median exceeds 1.2 times Mole's.

### M2 Status

- [ ] Every metric in `mac-analyze-status-tui.md` section B.B, with native APIs where Mole shells out (`host_processor_info`, `sysctlbyname`, IOKit for the battery).
- [ ] The health score and diagnosis line follow section B.C. Mole never collects CPU temperature on macOS, so its thermal branch never fires. The first M2 PR checks whether SMC temperatures are readable without root, and the thermal term ships only if they are.
- [ ] Perf. Idle CPU of `dustpan status` against `status-go` over 60 seconds. Fail above 1.2 times.

### M3 Clean (user)

This is the largest milestone, because the catalog is curated data. Split it into one PR per catalog section: user essentials, app caches, browsers, cloud and office, developer tools (four PRs), apps, virtualization, Application Support, leftovers, and review-only rows. Each PR ships its TOML rows, one fixture per incident the rows cite, and a parity run.

- [ ] Review, Progress and Summary widgets, shared by all later flows. Confirm comes from M1.
- [ ] `clean --dry-run` writes the preview list. `--json` emits the same items.
- [ ] The protected-paths editor in Settings and `clean --whitelist`. Hard-safety patterns always merge back in. An unreadable whitelist file stops the run.
- [ ] The History screen in the TUI, over the `history` command M0-5 built.
- [ ] Every incident in `safety-invariants.md` section 7 has a fixture test that fails without its guard.

### M4 Clean (admin) and optimize

- [ ] The privileged helper from spike S3, with the closed `PrivilegedAction` enum and per-action previews.
- [ ] The system catalog C-SYS-1 to C-SYS-7, orphaned launch daemons and helpers, Time Machine `.inProgress` bundles, and `.ipsw` firmware.
- [ ] 19 optimize tasks as a registry of `Task { id, needs_admin, run }`, each returning one `Outcome`, with the VPN guard on network tasks.
- [ ] Performance diagnosis shared by `optimize` and `status`.
- [ ] `touchid` writes `pam_tid.so` to `/etc/pam.d/sudo_local` through `install -m 444 -o root -g wheel`.

### M5 Uninstall

- [ ] App discovery across `/Applications`, `~/Applications`, input methods, PlayCover and mounted volumes, with display-name resolution in the order of porting note 16.
- [ ] Leftover matching by reverse-DNS id with boundary matching at `.` only. Personal data is kept by default, and `/Library` rows are review-only.
- [ ] Same-bundle-id siblings demote every match (porting note 6).
- [ ] Homebrew casks uninstall through `brew uninstall --cask` without `--zap`.

### M6 Purge and installers

- [ ] Purge discovery, the 33 artifact names plus `CACHEDIR.TAG`, the 7-day activity rule, and the per-type protection in `mac-clean-purge-installer.md` section 3.
- [ ] Installer detection, including zip listing checks, with Trash as the default sink.

### M7 Slim and lifecycle

- [ ] Slim parses fat headers itself, thins a clone, and verifies deep codesign, Team ID and Gatekeeper before it swaps (porting notes 21 to 25).
- [ ] `completion` for bash, zsh and fish from `clap_complete`; `update` with stable and nightly channels; `remove`.
- [ ] Release builds for `aarch64-apple-darwin` and `x86_64-apple-darwin`, a Homebrew tap, and signed and notarized binaries.

### M8 Windows

- [ ] `dustpan-sys::windows` provides file identity from `GetFileInformationByHandle`, the Recycle Bin through `IFileOperation`, process probes, `\\?\` long paths, and junction-safe walks (`windows.md` sections 7.1 to 7.8).
- [ ] The Windows catalog from `windows.md` section 3.3, including the `--system`, `--game-media` and `--gpu-shaders` groups.
- [ ] Uninstall from the four registry roots and MSIX, then the install folder to the Recycle Bin under the ownership rules in section 4.3.
- [ ] The Windows optimize steps from WIN-OPT-01..10, and the status collectors.

### M9 Linux

- [ ] `dustpan-sys::linux` provides freedesktop Trash ported from `internal/xdgtrash`, `/proc` and `/sys` metrics, and process probes that handle the 15-byte `comm` limit.
- [ ] The rules table from `linux.md` section 3, the package-manager actions in section 3.4 as `PrivilegedAction` variants, and Flatpak and Snap owners.
- [ ] Uninstall from `.desktop` files and package ownership with exact-id leftovers.

## Appendix A. Spike questions for M0

M0-2 ran each spike on a throwaway branch on one Mac (Apple M1 Pro, macOS 26.6.2). The verdicts are in the second table, and each note in `docs/spikes/` holds the numbers and the scripts that produced them. Steps that need a fingerprint, a password, a UAC click, a second machine or Remote Login are pending the operator, and each note lists them.

| Spike | Question | Decides |
|---|---|---|
| S1 Scan engine | Is a native parallel walk (`jwalk` plus `getattrlistbulk`) faster than Mole's `du -skPx` workers on a real home folder, with the same byte totals? | Whether the scanner shells out to `du` or walks natively |
| S2 macOS Trash | Which of `/usr/bin/trash`, `NSFileManager trashItem` through `objc2`, `renameatx_np` into `~/.Trash`, and Finder AppleScript works over SSH, inside `~/Library/Containers`, and with Put Back? | The `Trash` sink on macOS |
| S3 Privilege helper | Can the TUI suspend, run `sudo -v` with Touch ID, start `dustpan --privileged-helper` over a pipe, and resume without leaking keystrokes? What does the Windows `runas` relaunch need? | The `Privileged` sink and M4's shape |
| S4 Terminal | Do sub-cell bars, braille spinners, emoji widths and mouse events render correctly in Terminal.app, iTerm2, Ghostty, Windows Terminal, conhost and a Linux VTE terminal? | Glyph set and ASCII fallback rules |
| S5 Parity harness | Can `mo clean`, `purge` and `installer` run in dry-run mode against a fixture `HOME` and produce a parseable candidate list? | Whether parity is a CI gate or a manual check |

| Spike | Verdict | Branch at SHA | Note |
|---|---|---|---|
| S1 | Walk natively with `getattrlistbulk` on a bounded `rayon` pool, not `du` workers: 0.29 to 0.30 times Mole's time on `~`, total within 0.98 percent of Mole's. M1 picks its APFS clone rule on purpose. | `spike/s1-scan` at `6ed92cc` | `docs/spikes/s1-scan.md` |
| S2 | The macOS `Trash` sink uses `NSFileManager trashItemAtURL` through `objc2-foundation`. A container folder owned by `containermanagerd` is a permission failure, never a delete. Put Back and SSH are pending the operator. | `spike/s2-trash` at `ed75e60` | `docs/spikes/s2-trash.md` |
| S3 | Suspend, `sudo -v` on the user's terminal, then `sudo -n dustpan --privileged-helper` over stdin and stdout with a closed action enum; flush input and drain events before resume. Touch ID, password and Windows runs are pending the operator. | `spike/s3-privilege` at `c54dd6a` | `docs/spikes/s3-privilege.md` |
| S4 | Box drawing, eighth blocks, braille spinner, `🧹`, bare `⚙` (U+2699), `➤ ▾ ▸ ✓ ✗ ■ › ·`; never `⚙️` with U+FE0F. ASCII fallback on `--ascii` or a non-UTF-8 locale, with every frame character mapped at the same width. iTerm2, Ghostty, Windows and VTE screenshots are pending the operator. | `spike/s4-terminal` at `f2a7c6a` | `docs/spikes/s4-terminal.md` |
| S5 | Parity is a CI gate on `macos-latest` (GitHub no longer runs `macos-14`), with a pinned Mole, a fresh fixture `HOME`, a scrubbed environment and the write-deny sandbox, comparing only paths inside the fixture. | `spike/s5-parity` at `594db77` | `docs/spikes/s5-parity.md` |

## Appendix B. Alternatives rejected

- **Wrap the `mo` binary in a Rust TUI.** It ships fastest, but it inherits 59,055 lines of Bash and 14,239 lines of Go on `main` (counted with `wc -l`, tests excluded for Go), cannot add JSON or typed guards, and does not reach Windows without PowerShell. The user also asked for a native Rust tool.
- **Port Mole's Bash function by function.** It keeps per-app special cases spread across files. The rules table and typed pipeline remove whole classes of Mole's incidents instead.
- **One crate.** It is simpler to start, but OS code and domain logic would mix, and core tests would need OS mocks. Three crates is the smallest split that keeps the core pure.
- **Async runtime everywhere.** Scanning and probing are blocking syscalls. Bounded thread pools (`rayon` or `std::thread` with channels) match the work. Mole's analyzer also caps workers at 12 because macOS kills a process that holds too many threads (`mac-analyze-status-tui.md` section 2.2).
- **Trash for every deletion.** It feels safer, but caches in the Trash free no space until the user empties it, and `node_modules` moves are slow. Each sink is now a per-rule decision that the confirm dialog shows.
- **Restore from history in v1.** Put Back on macOS needs Finder scripting, and Windows and Linux restore differently. "Reveal in Trash" ships first, and restore waits for user demand.
- **A permissive license.** This plan rejects it unless the operator funds a clean-room catalog. See "Decide these before M0 starts".

## Appendix C. Risks

| Risk | Lands in | What the owner watches |
|---|---|---|
| The catalog drifts from Mole, which ships weekly | M3 to M6 | Pin a Mole SHA per catalog PR and rerun the inventory gate when the pin moves |
| A rule deletes user data that looks like a cache | M3 | One fixture per incident in `safety-invariants.md` section 7, row 1 |
| Running-app checks miss an owner | M3, M5 | `OwnerState::Unknown` counts as running, and a test exists for each unknown path |
| The privileged helper widens what root can touch | M4 | A closed `PrivilegedAction` enum, the helper re-runs the guard, and no environment variable changes a target (incident I-64) |
| macOS TCC and Full Disk Access prompts break scans | M1, M3 | Permission-only failures are classified as `Partial`, never as absence |
| Windows long paths and junctions | M8 | `\\?\` everywhere and fixtures over 260 characters |
| Linux distro spread | M9 | Detect tools, not distro ids; ship apt and pacman, list dnf and zypper as follow-up |
| The perf gates compare against Mole on one machine only | M1, M2 | Record the machine, OS version and cache state with every number |
| A recursive delete crosses into a mount inside the target | M0 | The sink checks the device at every level |

## Appendix D. Links and reading list

- `docs/inventory/mac-clean-purge-installer.md`, `mac-uninstall-slim.md`, `mac-optimize-core-cli.md`, `mac-analyze-status-tui.md`, `safety-invariants.md`, `windows.md`, `linux.md`. Read the matching file before you build a milestone.
- `scripts/inventory-gate.sh` and `scripts/README.md`. Rerun the gate whenever the Mole pin moves.
- Mole sources at `../mole` (`main`), with `origin/windows` and `origin/linux`. Mole's `docs/incidents.md` and `.claude/skills/bugs/references/` hold the incident detail behind the safety rules.
- M0-3 and M4 get the `architect` skill before code. Every PR that touches the sink or the guard gets the `interrogate` skill.
- `docs/decisions.md` holds the decision trail for this plan.
