# Windows inventory (Mole CLI, branch `windows`, SHA 59f3dc5a)

Source tree read: `a temporary extract of `origin/windows` from `../mole``. All paths below are relative to it. Nothing was executed.

Conventions.
- `file:line` points at the line where the claim is visible. A range means the claim spans it.
- Privilege. Mole never elevates itself. A search of `bin`, `lib`, `install.ps1`, `quick-install.ps1`, `mole.ps1` and `cmd` for `RunAs`, `-Verb` and `requireAdministrator` found no match. "admin" below means "the step runs only if the shell is already elevated, otherwise it is skipped" (`lib/core/base.ps1:258-266`, `bin/clean.ps1:268-279`). "UAC" appears only for vendor uninstallers, which request elevation themselves (`AGENTS.md:124`).
- macOS equivalent. This tree has no macOS source. Values come only from `SYNC.md` and in-tree comments. `n/s` means "not stated in SYNC.md, unverified from this tree"; check it against the macOS inventory.
- There is no shell-completion script in this tree (file listing of the extracted branch).
- Counting rule. Commands are the entries of `$validCommands`. Flags are the parameters each `bin/*.ps1` declares, because the router refuses any flag the target script does not declare (`mole.ps1:329-335`).

## 1. Commands and global flags

### 1.1 Commands

The router accepts 10 commands (`mole.ps1:458`), plus the `analyse` alias (`mole.ps1:438`) and a bare `mo` that opens a 5-item menu (`mole.ps1:197-254`, `mole.ps1:475-489`).

| # | Command | Script | Declared at |
|---|---|---|---|
| 1 | `clean` | `bin/clean.ps1` | `mole.ps1:458`, `bin/clean.ps1:6-27` |
| 2 | `uninstall` | `bin/uninstall.ps1` | `bin/uninstall.ps1:6-20` |
| 3 | `analyze` (alias `analyse`) | `bin/analyze.ps1` then `analyze.exe` | `bin/analyze.ps1:6-17`, `mole.ps1:438` |
| 4 | `status` | `bin/status.ps1` then `status.exe` | `bin/status.ps1:6-15` |
| 5 | `optimize` | `bin/optimize.ps1` | `bin/optimize.ps1:6-15` |
| 6 | `purge` | `bin/purge.ps1` | `bin/purge.ps1:6-18` |
| 7 | `installer` | `bin/installer.ps1` | `bin/installer.ps1:7-16` |
| 8 | `history` | `bin/history.ps1` | `bin/history.ps1:6-17` |
| 9 | `update` | `bin/update.ps1` | `bin/update.ps1:5-11` |
| 10 | `remove` | `bin/remove.ps1` | `bin/remove.ps1:6-12` |

The menu lists clean, uninstall, optimize, analyze, status; keys are arrows or `j`/`k`, `1`-`5`, Enter, `M` for help, `V` for version, `Q` or Esc to quit (`mole.ps1:199-205`, `mole.ps1:232-248`). The menu runs one command and returns to the shell (`mole.ps1:474`).

### 1.2 Global flags

3 global flags.

| Flag | Behavior | Source |
|---|---|---|
| `-v`, `--version` | Prints Mole version, Windows name and version, native architecture (read from the registry so x64-on-ARM64 reports correctly), PowerShell version, free space, install method | `mole.ps1:13-14`, `mole.ps1:51-72`, `mole.ps1:433-444` |
| `-h`, `--help` | Main help table | `mole.ps1:16-17`, `mole.ps1:148-188`, `mole.ps1:447-450` |
| `--debug` | Opens `~\.config\mole\mole_debug_session.log` before the command runs. The `.cmd` launcher binds `--debug` as a PowerShell common parameter, so the router re-adds it | `mole.ps1:23-29`, `mole.ps1:371-374`, `lib/core/log.ps1:22-28`, `SYNC.md:209` |

Environment switches the commands read.

| Variable | Effect | Source |
|---|---|---|
| `MOLE_DRY_RUN=1` | Dry run for clean, optimize, remove (`remove` through `install.ps1 -DryRun`) | `bin/clean.ps1:361-367`, `bin/optimize.ps1:752`, `bin/remove.ps1:71`, `SYNC.md:217` |
| `MO_NO_OPLOG=1` | Turns off `operations.log` | `lib/core/log.ps1:62` |
| `MOLE_DEBUG=1` | Debug on | `lib/core/log.ps1:27`, `lib/core/log.ps1:437` |
| `MO_ANALYZE_PATH` | Path handed from `analyze.ps1` to the Go binary | `bin/analyze.ps1:72-73`, `cmd/analyze/main_windows.go:18` |

Router behavior worth copying.
- An unknown command exits 2 (`mole.ps1:465-469`). An unknown flag is refused with exit 2 and is not forwarded, because scripts without `[CmdletBinding()]` ignore unknown parameters and `mo remove --help` once uninstalled Mole (`mole.ps1:260-263`, `mole.ps1:330-335`).
- `--help` and `--debug` map to `ShowHelp` and `DebugMode` for every command (`mole.ps1:282`).
- A flag takes the next word as its value only if the script declares a non-switch parameter and the next word is not another flag (`mole.ps1:336-345`). `--name=value` and `--name:value` also work (`mole.ps1:346-357`).
- Only analyze, status, history, uninstall, purge, installer, clean and remove return their script's exit code. `optimize` and `update` do not (`mole.ps1:310`, `mole.ps1:386-388`).
- Unknown command exit is 2 here and 1 on the other CLIs, an intentional difference (`SYNC.md:217`).

### 1.3 Per-command flags

37 declared parameters (20 excluding the repeated `--help` and `--debug`).

| Command | Declared flags | Source |
|---|---|---|
| clean | `--dry-run`, `-s/--system`, `--game-media`, `--gpu-shaders`, `-d/--debug`, `-w/--whitelist`, `-h/--help` | `bin/clean.ps1:6-27` |
| uninstall | `--dry-run`, `-d/--debug`, `-r/--rescan`, `--list`, `-h/--help` | `bin/uninstall.ps1:6-20` |
| optimize | `--dry-run`, `-d/--debug`, `-h/--help` | `bin/optimize.ps1:6-15` |
| purge | `--dry-run`, `-d/--debug`, `-p/--paths`, `-h/--help` | `bin/purge.ps1:6-18` |
| installer | `--dry-run`, `-d/--debug`, `-h/--help` | `bin/installer.ps1:7-16` |
| remove | `--dry-run`, `-h/--help` | `bin/remove.ps1:6-12` |
| history | `-h/--help`, `--json`, `--limit N` (1-200, also `--limit=N`) | `bin/history.ps1:6-17`, `lib/core/history.ps1:10-11`, `lib/core/history.ps1:14-21` |
| analyze | `[path]`, `--json`, `-d/--debug`, `-h/--help` | `bin/analyze.ps1:6-17` |
| status | `--json`, `--watch`, `--interval=<dur>`, `-h/--help` | `bin/status.ps1:6-15` |
| update | `-d/--debug`, `-h/--help` | `bin/update.ps1:5-11` |

Mismatches between `mo` and the Go binaries.
- `status.exe` also defines `--proc-cpu-threshold`, `--proc-cpu-window` and `--proc-cpu-alerts` (`cmd/status/main.go:27-29`), but `status.ps1` never forwards them, so `mo status --proc-cpu-threshold` is refused by the router (`mole.ps1:329-335`). They exist only when `status.exe` runs directly.
- `analyze.exe` also accepts a legacy `-path` flag (`cmd/analyze/cli.go:44`), unreachable through `mo` for the same reason. Flags must precede the path argument (`cmd/analyze/cli.go:36-39`).
- `status.ps1` declares no `DebugMode`, so `mo status --debug` is refused (`bin/status.ps1:6-15`, `mole.ps1:281-290`).

## 2. Feature table

Safety rule column abbreviations. "Guard" means the shared `Test-SafePath` plus whitelist plus path identity snapshot chain (`lib/core/file_ops.ps1:105-158`, `lib/core/file_ops.ps1:32-103`). Deletion sinks are detailed in section 7.

### 2.1 Router and shell

| ID | Command | Flags/options | Behavior | Privilege | Safety rule | macOS equivalent | Source |
|---|---|---|---|---|---|---|---|
| WIN-CORE-01 | `mo` | none | Arrow-key menu with five commands, update notice line | user | Notice only when output is interactive, not dry run, not json/help/list | n/s (menu text "as the macOS CLI's main menu", `mole.ps1:194`) | `mole.ps1:197-254`, `lib/core/update_check.ps1:4-13` |
| WIN-CORE-02 | `mo --version` | `-v` | Version block | user | none | n/s | `mole.ps1:51-60` |
| WIN-CORE-03 | `mo --help` | `-h` | Help table | user | none | n/s | `mole.ps1:148-188` |
| WIN-CORE-04 | any | `--debug` | Debug session log, header names no user or machine | user | Log write failure never stops the command | same shape (`SYNC.md:209`) | `mole.ps1:75-90`, `SYNC.md:209` |
| WIN-CORE-05 | `mo analyse` | alias | Routes to analyze | user | none | same (`SYNC.md:217`) | `mole.ps1:438` |
| WIN-CORE-06 | any | unknown flag | Refused, exit 2 | user | Never forwarded to the script | n/s | `mole.ps1:330-335` |
| WIN-CORE-07 | state folders | none | Config `~\.config\mole`, cache `~\.cache\mole`, created at start, creation failure tolerated | user | Preview must run where state cannot be written | n/s | `lib/core/base.ps1:153-162`, `lib/core/base.ps1:762-785`, `SYNC.md:104` |

### 2.2 Clean

| ID | Command | Flags/options | Behavior | Privilege | Safety rule | macOS equivalent | Source |
|---|---|---|---|---|---|---|---|
| WIN-CLEAN-01 | `mo clean` | none | Runs fixed stages in order: user essentials, browser caches, app caches, dev tools, applications. No selection list and no confirmation prompt. | user | Guard on every entry; clean-only protected segments on (`$script:MoleCleanPathGuard`) | differs. macOS has review sections the Windows CLI lacks (`SYNC.md:164`) | `bin/clean.ps1:290-320`, `bin/clean.ps1:50`, `bin/clean.ps1:341-375` |
| WIN-CLEAN-02 | `mo clean` (user essentials) | none | `%TEMP%` top-level files older than 7 days (never enters subfolders); WER `ReportArchive`, `ReportQueue` and `CrashDumps` `.dmp` files older than 7 days; `%LOCALAPPDATA%\Temp\*.log` and `%APPDATA%\*.log` older than 7 days | user | Age is the newer of write and creation time; a WER report is kept whole if any descendant is recent or unreadable; bounded 4096 entries and 2 s | n/s | `lib/clean/user.ps1:21-34`, `lib/clean/user.ps1:42-98`, `lib/clean/user.ps1:104-130`, `lib/core/file_ops.ps1:841-877`, `SYNC.md:87` |
| WIN-CLEAN-03 | `mo clean` (browsers) | none | Chromium family cache leaves per profile (Chrome, Beta, Canary, Edge, Beta, Dev, Brave, Vivaldi, 360 Chrome, Quark, BrowserOS), Firefox `cache2`, `startupCache`, `shader-cache`, Opera | user | Only named cache leaves. Skipped while the browser process runs or its lockfile is held. Profile discovery one level, max 128 dirs, 2 s | differs (Windows paths only) | `lib/clean/caches.ps1:22-83`, `lib/clean/cache_guard.ps1:6-67` |
| WIN-CLEAN-04 | `mo clean` (app caches) | none | Discord (stable, PTB, Canary), Vesktop, Slack, Teams classic and new, Cursor, Windsurf, Obsidian, Notion, Kiro, CodeBuddy, Qoder, Trae, T3 Code, QQ, Feishu, Lark, Jianying, CapCut, VS Code, Epic webcache; week-old logs for Tencent Meeting, Baidu Netdisk, Docker Desktop, WPS | user | `Clear-MoleAppCache` with the owner's process names; `CacheStorage`, `Local Storage`, `Cookies` and similar segments are protected | differs (app list is Windows-specific) | `lib/clean/caches.ps1:89-216`, `lib/core/base.ps1:219-248`, `.claude/rules/clean.md:23-24` |
| WIN-CLEAN-05 | `mo clean` (dev tools) | none | npm, pnpm, pip, Go, mise, Bun through the owner's own command; Yarn Classic, pyenv, Poetry, node-gyp, Electron, TypeScript, NuGet HTTP and plugins cache, MSBuild temp, Cargo registry and git checkouts, rustup downloads, Hex, Opam, Stack temp, gcloud logs, Terraform plugin cache, Zed, Sublime, Atom, VS Code and Insiders, Visual Studio caches, JetBrains `caches`/`index`/`tmp`, GitHub CLI json older than 7 days | user | Owner command pipeline re-queries the root, rechecks whitelist and snapshot, times out at 120 s, never falls back to a guessed directory; Docker, JVM and Mix archives are no-ops | differs (n/s per tool) | `lib/clean/dev.ps1:25-178`, `lib/clean/dev.ps1:180-742`, `.claude/rules/clean.md:20` |
| WIN-CLEAN-06 | `mo clean` (applications) | none | Office `Wef` and Outlook offline address books and `*.tmp`, OneDrive setup logs, Google Drive logs and `*.tmp`, Adobe media cache and peak files, `Adobe\*\Cache`, Autodesk caches, launchers (Steam htmlcache, Epic, EA app, Origin, GOG Galaxy, Ubisoft Connect, Battle.net) | user | Launcher caches only while the launcher is closed; logs at 7 days; Adobe Team Projects Cache protected | differs | `lib/clean/apps.ps1:22-218`, `lib/clean/apps.ps1:448-478`, `SYNC.md:81` |
| WIN-CLEAN-07 | `mo clean --dry-run` | `--dry-run`, env `MOLE_DRY_RUN` | Lists each target with its size and writes every path to `~\.config\mole\clean-list.txt` for the whitelist | user | Writes nothing else; no log line; list failure tolerated | same flag (`SYNC.md:104`); dry-run logging differs (`SYNC.md:210`) | `bin/clean.ps1:56`, `bin/clean.ps1:235-261`, `bin/clean.ps1:361-367`, `SYNC.md:210` |
| WIN-CLEAN-08 | `mo clean --system` | `-s/--system` | Adds `Invoke-SystemCleanup`: `%PROGRAMDATA%` WER queue and archive at 7 days, `MSI*.LOG` in `%TEMP%` at 30 days, `%LOCALAPPDATA%\CrashDumps` contents, `C:\NVIDIA\DisplayDriver` version folders where every entry is a week old, Delivery Optimization files | admin (skipped with a warning when not elevated) | Delivery Optimization is never deleted by Mole: it asks the service via CIM `MSFT_DeliveryOptimizationFile.Delete` for finished, unpinned files only; NVIDIA folders need no process running from them | Windows-only | `bin/clean.ps1:268-279`, `bin/clean.ps1:317-320`, `lib/clean/system.ps1:23-46`, `lib/clean/system.ps1:52-81`, `lib/clean/system.ps1:119-159`, `lib/clean/system.ps1:165-312`, `.claude/rules/clean.md:28` |
| WIN-CLEAN-09 | `mo clean --game-media` | `--game-media` | Files older than 90 days from NVIDIA ShadowPlay and Highlights, AMD ReLive, Xbox captures, Steam screenshots, OBS, Game DVR, Medal, Overwolf, and replay folders for Fortnite, League, Valorant, Rocket League, Call of Duty, Apex | user | Never touches generic `Pictures\Screenshots` or `Videos\Screen Recordings` | Windows-only | `bin/clean.ps1:309-315`, `lib/clean/apps.ps1:224-442`, `.claude/rules/clean.md:19` |
| WIN-CLEAN-10 | `mo clean --gpu-shaders` | `--gpu-shaders` | NVIDIA DXCache, GLCache, NV_Cache, AMD DXCache, GLCache, VkCache, Intel ShaderCache, D3DSCache, DirectX Shader Cache, VulkanCache | user | Opt-in because every game recompiles shaders on next launch; browser shader caches stay in the default run | Windows-only | `bin/clean.ps1:297-301`, `lib/clean/caches.ps1:222-290`, `SYNC.md:213` |
| WIN-CLEAN-11 | `mo clean --whitelist` | `-w/--whitelist` | Creates `~\.config\mole\whitelist.txt` if missing and opens it in `notepad.exe` and waits | user | Whitelist read once per removal; an unreadable file retains every target | differs (editor, not a TUI; macOS interaction n/s) | `bin/clean.ps1:91-122`, `lib/core/base.ps1:600-627` |
| WIN-CLEAN-12 | `mo clean` (exit) | none | Before a real run, creates and removes a probe folder in `%LOCALAPPDATA%` and `%APPDATA%`; if every folder refuses a write, prints the reason and exits 1. Otherwise exits 0 even when locked files were skipped | user | A sandboxed shell cannot report "already clean" | same behavior on macOS (`SYNC.md:105`) | `bin/clean.ps1:164-216`, `bin/clean.ps1:370-372`, `bin/clean.ps1:380`, `SYNC.md:105` |

### 2.3 Uninstall

| ID | Command | Flags/options | Behavior | Privilege | Safety rule | macOS equivalent | Source |
|---|---|---|---|---|---|---|---|
| WIN-UNIN-01 | `mo uninstall` | none | Scans four registry Uninstall roots plus Store packages, sorts by size descending, shows a paged multi-select (Space, Enter, `/` search, PageUp/PageDown, Esc/Q), then a `Continue? (y/N)` prompt | user (the vendor uninstaller may raise UAC) | Menu rows never authorize removal; selection keyed by registry key or package full name, not display name | differs (n/s) | `bin/uninstall.ps1:864-1007`, `bin/uninstall.ps1:1017-1189`, `bin/uninstall.ps1:1556`, `.claude/rules/uninstall.md:16` |
| WIN-UNIN-02 | `mo uninstall --list` | `--list` | Prints the inventory and exits; JSON array when stdout is redirected, text otherwise. Fields name, id, source, version, publisher, path, size, size_bytes | user | Runs no uninstaller, asks for no elevation, writes no log line | same JSON fields where a fact exists on both (`bin/uninstall.ps1:1200-1201`) | `bin/uninstall.ps1:1195-1254` |
| WIN-UNIN-03 | `mo uninstall --rescan` | `-r/--rescan` | Ignores the 24-hour inventory cache `app_scan_cache_v7.json` | user | A cache stamped in the future counts as expired | n/s | `bin/uninstall.ps1:46-47`, `bin/uninstall.ps1:872-887`, `SYNC.md:189` |
| WIN-UNIN-04 | `mo uninstall --dry-run` | `--dry-run` | Prints the method per app (`msiexec /x`, vendor command, `Remove-AppxPackage`) and whether a leftover folder would be recycled. Without a console it prints the `--list` inventory and exits 0 | user | Writes nothing | same (`SYNC.md:216`) | `bin/uninstall.ps1:1261-1280`, `bin/uninstall.ps1:1341-1360`, `bin/uninstall.ps1:1509-1515`, `SYNC.md:216` |
| WIN-UNIN-05 | uninstall run | n/a | Runs each registered uninstaller once: `msiexec /x {code} /passive REBOOT=ReallySuppress /norestart` for MSI, the registered `QuietUninstallString`, `/SILENT /SUPPRESSMSGBOXES /NORESTART` for detected Inno, `/S` for detected NSIS, else the registered command | user, UAC per vendor | Registration re-read right before launch; a changed command is logged `FAILED` and not run; 30-minute wait ceiling then the batch stops and the vendor is not killed | Windows-only | `bin/uninstall.ps1:633-765`, `bin/uninstall.ps1:775-811`, `bin/uninstall.ps1:1417-1422`, `.claude/rules/uninstall.md:19-20` |
| WIN-UNIN-06 | uninstall run (Store) | n/a | `Remove-AppxPackage -Package <full name>` | user | Exact current package re-read and manifest checked at preview and at removal; unreadable manifest retains the app | Windows-only | `bin/uninstall.ps1:538-567`, `bin/uninstall.ps1:1382-1391` |
| WIN-UNIN-07 | uninstall run (leftovers) | n/a | After the uninstall registry key is gone, moves the one registered install folder to the Recycle Bin | user | `Test-MoleLeftoverOwnership` (section 4.3); no permanent-delete fallback | differs. macOS scans many leftover locations (section 8) | `bin/uninstall.ps1:1445-1451`, `bin/uninstall.ps1:307-328`, `.claude/rules/uninstall.md:21` |
| WIN-UNIN-08 | uninstall run (exit) | n/a | Exit 1 when any app fails, keeps its entry, or is an ineligible Store package | user | n/a | macOS also exits 1 on failure (`SYNC.md:112`) | `bin/uninstall.ps1:1468-1473`, `bin/uninstall.ps1:1570`, `SYNC.md:112` |

### 2.4 Optimize

| ID | Command | Flags/options | Behavior | Privilege | Safety rule | macOS equivalent | Source |
|---|---|---|---|---|---|---|---|
| WIN-OPT-01 | `mo optimize` (disk) | none | `Optimize-Volume` on the system drive (Windows default), then `-ReTrim` on other lettered fixed NTFS or ReFS volumes | admin | Skipped on unhealthy target disk, on battery or unknown power; extra volumes need Healthy volume, partition count 1, Healthy SSD; a hard disk is never defragmented | differs (no `Optimize-Volume` on macOS, `SYNC.md:88`) | `bin/optimize.ps1:144-274`, `.claude/rules/optimize.md:15-16`, `SYNC.md:88` |
| WIN-OPT-02 | `mo optimize` (DNS) | none | `Clear-DnsClientCache` | user | n/a | n/s | `bin/optimize.ps1:276-303` |
| WIN-OPT-03 | `mo optimize` (network) | none | `netsh interface ip delete arpcache` | admin | Checks `$LASTEXITCODE` | n/s | `bin/optimize.ps1:305-339` |
| WIN-OPT-04 | `mo optimize` (startup) | none | Counts entries in `HKCU`/`HKLM`/`WOW6432Node` `Run` keys and the Startup folder; read-only | user | Count never read as a fault | n/s | `bin/optimize.ps1:341-381` |
| WIN-OPT-05 | `mo optimize` (disk health) | none | `Get-PhysicalDisk` health per disk; gates WIN-OPT-01 | user | A target disk reporting Warning or Unhealthy blocks disk writes | n/s | `bin/optimize.ps1:591-651`, `bin/optimize.ps1:773-775` |
| WIN-OPT-06 | `mo optimize` (Windows Update) | none | Local-catalog `IsInstalled=0` search in a job with 5 s timeout, reports important and optional counts | user | Timeout or incomplete result is "unknown", never zero | Windows-only | `bin/optimize.ps1:653-713`, `.claude/rules/optimize.md:18` |
| WIN-OPT-07 | `mo optimize` (system image) | none | `Repair-WindowsImage -Online -CheckHealth`; prints the `DISM /RestoreHealth` command but never runs it | admin | Repair is never run (it downloads and takes up to 30 min) | Windows-only | `bin/optimize.ps1:542-589`, `SYNC.md:208` |
| WIN-OPT-08 | `mo optimize` (SFC) | none | Prompts `Run System File Checker? (y/N)` only in a real elevated run; `sfc /scannow` output redirected to a unique temp file, UTF-16 decoded, verdict from text or from `CBS.log` `[SR]` lines | admin | Success recorded only with exit 0 and a recognized verdict | Windows-only | `bin/optimize.ps1:383-458`, `bin/optimize.ps1:460-540`, `bin/optimize.ps1:786-793`, `SYNC.md:208` |
| WIN-OPT-09 | `mo optimize --dry-run` | `--dry-run` | Each step prints "Would ..." | user | Reads health first, even in preview | same flag (`SYNC.md:104`) | `bin/optimize.ps1:752`, `bin/optimize.ps1:762-765`, `SYNC.md:104` |
| WIN-OPT-10 | `mo optimize` (exclusions) | n/a | Deliberately does not reset Winsock or rebuild the Search index, icon, font or Store caches | n/a | An elevated run once reset Winsock and deleted the Search index (incident I-03) | n/s | `.claude/rules/optimize.md:17`, `docs/incidents.md:34` |

### 2.5 Analyze

| ID | Command | Flags/options | Behavior | Privilege | Safety rule | macOS equivalent | Source |
|---|---|---|---|---|---|---|---|
| WIN-ANA-01 | `mo analyze` | none | Overview of Home, User AppData, Program Files, Program Files (x86), ProgramData, "insight" rows, then each fixed drive | user | Fixed drives only, via `GetDriveType` | same layout intent (`README.md:173`) | `cmd/analyze/overview.go:20-51`, `cmd/analyze/platform_windows.go:108-134`, `README.md:173` |
| WIN-ANA-02 | `mo analyze <path>` | positional path | Scans an existing path; `D:\` scans a drive | user | Path must exist | same (`SYNC.md:217` for `analyse`) | `cmd/analyze/cli.go:62-79`, `bin/analyze.ps1:72-73` |
| WIN-ANA-03 | `mo analyze --json` | `--json` | Prints one JSON object and exits; cannot delete or write the operations log | user | JSON mode never starts the TUI | n/s | `cmd/analyze/main_windows.go:28-36`, `cmd/analyze/json.go:20-43` |
| WIN-ANA-04 | `mo analyze` keys | n/a | Up/Down or `j`/`k`, Enter/Right/`l` open, Left/`h`/`b`/Backspace back, Space select, `T` top large files, `/` filter, `O` open, `F` show in Explorer, `P` preview, `R` refresh, `S` live sort during scan, Delete, `Q` | user | Delete blocked while scanning | same layout (`README.md:173`) | `cmd/analyze/update.go:575-893`, `bin/analyze.ps1:40-51` |
| WIN-ANA-05 | `mo analyze` Delete | Delete key, Enter to confirm | Moves selection to the Recycle Bin through `SHFileOperationW` | user | See 7.3 and the protection list in 3.6 | n/s | `cmd/analyze/update.go:893-950`, `cmd/analyze/view.go:408-434`, `cmd/analyze/platform_windows.go:280-329` |
| WIN-ANA-06 | `mo analyze` sizes | n/a | Sizes by occupied bytes. Online-only OneDrive placeholders count 0; sparse, compressed and partly downloaded files use `GetCompressedFileSizeW`; last access is blank when NTFS does not record it | user | Never opens an online-only file | Windows-only | `cmd/analyze/disk_size.go:4-38`, `cmd/analyze/platform_windows.go:23-31`, `cmd/analyze/platform_windows.go:350-371`, `AGENTS.md:99` |
| WIN-ANA-07 | `mo analyze` hints | n/a | "Cleanable" only when a directory carries a valid `CACHEDIR.TAG`; folders handled by `mo clean` excluded; folding list of build and cache folder names | user | A directory name is not evidence | n/s | `cmd/analyze/cleanable.go:15-70`, `cmd/analyze/constants.go:33-131` |

### 2.6 Status

| ID | Command | Flags/options | Behavior | Privilege | Safety rule | macOS equivalent | Source |
|---|---|---|---|---|---|---|---|
| WIN-STAT-01 | `mo status` | none | Alt-screen dashboard with health score, CPU, memory, disk, power, network, processes. Keys `c` cycle core rows, `k` toggle the mole, `q`/Esc/Ctrl+C quit | user | Read-only | same card set, fewer fields (`cmd/status/metrics.go:53-55`) | `cmd/status/main.go:113-128`, `cmd/status/main.go:303-309`, `bin/status.ps1:26-48` |
| WIN-STAT-02 | `mo status --json` | `--json` | One snapshot; JSON also when stdout is not a terminal | user | n/a | same contract for the fields Windows can measure (`cmd/status/metrics.go:53-55`) | `cmd/status/main.go:36`, `cmd/status/main.go:256-290`, `cmd/status/metrics.go:56-80` |
| WIN-STAT-03 | `mo status --watch --interval` | `--watch`, `--interval=<dur>` | Newline-delimited JSON, default 1 s | user | Interval parsed and validated | n/s | `cmd/status/main.go:32-33`, `cmd/status/main.go:311-351` |
| WIN-STAT-04 | health score | n/a | Weights CPU 30, memory 25, disk 20, disk IO 10; battery, uptime and failing-SMART penalties; no thermal term on Windows | user | n/a | same formula minus thermal (`cmd/status/metrics_health.go:52-54`) | `cmd/status/metrics_health.go:8-191` |
| WIN-STAT-05 | process alerts | flags only on `status.exe` | Persistent alert when a process stays above a CPU threshold for a window (default 100 percent of one logical CPU, 5 min) | user | n/a | CPU denominator differs from Mole for Windows only (`SYNC.md:205`) | `cmd/status/main.go:27-29`, `cmd/status/process_watch.go`, `SYNC.md:205` |

### 2.7 Purge, installer, history

| ID | Command | Flags/options | Behavior | Privilege | Safety rule | macOS equivalent | Source |
|---|---|---|---|---|---|---|---|
| WIN-PURGE-01 | `mo purge` | none | Scans default roots (`Documents`, `Projects`, `Code`, `Development`, `Developer`, `workspace`, `github`, `repos`, `src`, `D:\Projects`, `D:\Code`, `D:\Development`) up to depth 4 below each root, marker files `package.json`, `Cargo.toml`, `go.mod`, `pom.xml`, `build.gradle`, `requirements.txt`, `pyproject.toml`, `.sln`/`.csproj`; multi-select, then `Continue? (y/N)` | user | Nothing preselected; custom paths file that exists but cannot be read stops the run | same command; depth 5 here vs 6 on macOS (`SYNC.md:224`) | `bin/purge.ps1:37-50`, `bin/purge.ps1:444-471`, `bin/purge.ps1:585-730`, `bin/purge.ps1:921`, `SYNC.md:83`, `SYNC.md:224` |
| WIN-PURGE-02 | `mo purge` artifacts | n/a | `node_modules`, `vendor`, `.venv`, `venv`, `__pycache__`, `.pytest_cache`, `target`, `build`, `dist`, `.next`, `.nuxt`, `.turbo`, `.parcel-cache`, `bin`, `obj`, `.gradle`, `*.log` | user | `bin`/`obj` need a `.csproj`-family file, `target` needs `Cargo.toml` or `pom.xml`, `vendor` needs `go.mod` or `composer.json`, `build`/`dist` need a build marker; `.idea` excluded | n/s | `bin/purge.ps1:55-73`, `bin/purge.ps1:241-270` |
| WIN-PURGE-03 | `mo purge --paths` | `-p/--paths` | Opens `~\.config\mole\purge_paths.txt` for editing | user | n/a | n/s | `bin/purge.ps1:156-197`, `bin/purge.ps1:52`, `bin/purge.ps1:822-826` |
| WIN-PURGE-04 | `mo purge` safety | n/a | Keeps Git-tracked content, nested repos, and any artifact a process uses (image path or command line) or any entry that refuses an exclusive delete-access open | user | Probe is read-only so dry run equals real run; unknown process list retains everything | same rule on macOS, `lsof` spellings differ (`SYNC.md:67-68`) | `bin/purge.ps1:275-325`, `bin/purge.ps1:330-442`, `.claude/rules/purge.md:12-13`, `SYNC.md:67-68` |
| WIN-PURGE-05 | `mo purge --dry-run` | `--dry-run` | Previews every project; no selection list without a console | user | no confirmation | same (`SYNC.md:216`) | `bin/purge.ps1:828-831`, `bin/purge.ps1:883-887`, `bin/purge.ps1:915-919`, `SYNC.md:216` |
| WIN-INST-01 | `mo installer` | none | Lists `.msi` files directly in `%USERPROFILE%\Downloads`, at least 14 days old, whose compound-file root carries the MSI class ID; multi-select with `A` for all; Recycle Bin | user | Max 4096 entries read, 128 largest opened; file opened with no sharing, any failure is "held"; nothing preselected; recheck of identity, size, both times at the sink | differs. macOS covers `.dmg`, `.pkg`, `.iso`, `.xip`, installer `.zip` in more folders (`SYNC.md:18`) | `bin/installer.ps1:33-46`, `bin/installer.ps1:129-226`, `bin/installer.ps1:231-292`, `bin/installer.ps1:308-394`, `SYNC.md:18`, `SECURITY_AUDIT.md:55` |
| WIN-INST-02 | `mo installer --dry-run` | `--dry-run` | Previews every found file without a console | user | n/a | same shape (`SYNC.md:18`) | `bin/installer.ps1:405-408`, `bin/installer.ps1:434-438` |
| WIN-HIST-01 | `mo history` | none | Recent sessions with action counts and recent operations from `~\.config\mole\operations.log`, newest first, default 20 | user | Log moves to `.old` past 5 MB | same command; JSON keys differ (`operations`, `recycled`, `uninstalled` vs `deletions`, `trashed`, `rebuilt`) | `bin/history.ps1:69-76`, `lib/core/history.ps1:10-11`, `lib/core/log.ps1:35-36`, `SYNC.md:211`, `SYNC.md:217` |
| WIN-HIST-02 | `mo history --json --limit N` | `--json`, `--limit` | JSON with `logs`, `limit`, `sessions`, `operations`; invalid argument exits 2 | user | n/a | n/s | `bin/history.ps1:44-67`, `lib/core/history.ps1:128-138`, `SYNC.md:211` |
| WIN-LOG-01 | operations log | n/a | Line `[time] [command run=id] ACTION path (detail)`; actions seen are `REMOVED`, `RECYCLED`, `SKIPPED`, `FAILED`, `OWNER`, `UNINSTALLED`, `OPTIMIZED`. Dry runs write nothing | user | A write failure never fails the removal; held file retried | same line format (`SYNC.md:210`); dry-run logging differs (`SYNC.md:210`) | `lib/core/log.ps1:55-93`, `SYNC.md:210` |

### 2.8 Lifecycle

| ID | Command | Flags/options | Behavior | Privilege | Safety rule | macOS equivalent | Source |
|---|---|---|---|---|---|---|---|
| WIN-UPD-01 | `mo update` | `-d/--debug` | For a git source install, `git pull --ff-only --tags origin <branch>`, then re-runs the installer when the revision changed or the last refresh did not finish, rebuilds or refetches the native tools | user | Refuses dirty tracked changes; winget or Scoop installs get that manager's command instead; non-git copies get reinstall instructions | n/s (macOS `mo update` is Mole self-update too, `AGENTS.md:35`) | `bin/update.ps1:20-38`, `bin/update.ps1:195-216`, `bin/update.ps1:286`, `.claude/rules/install-update.md:23` |
| WIN-UPD-02 | update notice | n/a | Background lookup of the install's own channel (GitHub tags, Scoop bucket, winget manifest dir), cached 1 day, failures 1 hour; never installs | user | Silent for redirected output, help, version, json, dry run | n/s | `lib/core/update_check.ps1:4-26`, `.claude/rules/install-update.md:24` |
| WIN-UPD-03 | release highlights | n/a | After a pull that changed `VERSION`, prints up to four changelog titles of the new `V<version>-windows` release | user | 3 s lookup; silent on failure | n/s (main has it, `SYNC.md:97`) | `bin/update.ps1:276-290`, `bin/update.ps1:364`, `SYNC.md:97` |
| WIN-REM-01 | `mo remove` | `--dry-run` | Prompts `Remove Mole from <dir>? (y/N)`, then runs `install.ps1 -Uninstall`: removes listed top-level files, PATH entry, shortcuts, `~\.cache\mole`, asks before removing config | user | Package-manager installs get `winget uninstall tw93.MoleCLI` or `scoop uninstall mole`; refuses unrecognized or protected install roots; keeps `operations.log*` | same command (`SYNC.md:217`) | `bin/remove.ps1:20-92`, `install.ps1:920-1090`, `install.ps1:1079`, `.claude/rules/install-update.md:21-22`, `SYNC.md:217` |
| WIN-DIST-01 | install | `-InstallDir`, `-AddToPath`, `-CreateShortcut`, `-Uninstall`, `-Force`, `-DryRun` on `install.ps1` | Default `%LOCALAPPDATA%\Programs\mole-cli`; writes `mole.cmd` and `mo.cmd`; user PATH edit; optional Start Menu shortcut | user | Refuses protected install roots and physical-path surprises | n/a | `install.ps1:6-16`, `install.ps1:138-205`, `install.ps1:284-308`, `install.ps1:386-415`, `install.ps1:503-619` |
| WIN-DIST-02 | native tools | n/a | `analyze.exe` and `status.exe` come from `analyze-windows-x64.exe` and `status-windows-x64.exe` in the GitHub release, verified against `SHA256SUMS.txt`, or are built with Go | user | Checksum mismatch rejects the download | n/a | `lib/core/tui_binaries.ps1:15-19`, `lib/core/tui_binaries.ps1:42-46`, `lib/core/tui_binaries.ps1:68-172`, `lib/core/tui_binaries.ps1:310`, `SYNC.md:26` |
| WIN-DIST-03 | channels | n/a | `quick-install.ps1` one-liner, Scoop (`mo.exe` plus `mole` alias, x64 only), winget portable zip (`mo` alias), release zip | user | Releases are signed; `releases/latest` stays on macOS | n/a | `quick-install.ps1:3`, `packaging/scoop/mole.json:6-22`, `packaging/winget/tw93.MoleCLI.installer.yaml:6-17`, `scripts/build-release.ps1:40`, `scripts/build-release.ps1:168-169`, `.github/workflows/release-windows.yml:231` |
| WIN-DIST-04 | `mo.exe` launcher | all args | Go program that finds `mole.ps1` next to itself (following winget's symlink) and runs it through `powershell.exe` with `-ExecutionPolicy Bypass`, falling back to `pwsh.exe`; Ctrl+C is ignored in the launcher | user | Every argument single-quoted; a host that started is never retried | n/a | `cmd/mo/main.go:1-6`, `cmd/mo/main.go:34`, `cmd/mo/main.go:45-58`, `cmd/mo/main.go:63-69`, `cmd/mo/main.go:84-111` |

## 3. Windows cleanup target catalog

### 3.1 Declared versus effective

A row can be declared in code and still never fire. Check each against `lib/core/base.ps1:167-195`.

- Default whitelist roots (never cleaned, `lib/core/base.ps1:167-179`). `LOCALAPPDATA\Microsoft\Windows\Explorer`, `...\Fonts`, `APPDATA\Microsoft\Windows\Recent`, `~\.vscode\extensions`, `~\.nuget`, `~\.cargo`, `~\.rustup`, `~\.m2\repository`, `~\.gradle\caches\modules-2\files-*`, `~\.ollama\models`, `LOCALAPPDATA\JetBrains`.
- Consequence (inferred, not run). The Cargo registry and git rows (`lib/clean/dev.ps1:350-365`), rustup downloads, and JetBrains `caches`/`index`/`tmp` rows (`lib/clean/dev.ps1:635-651`) sit under default whitelist roots and are blocked by `Test-Whitelisted`. `SYNC.md:86` says these rows "stay" because the user can edit those roots, while `Get-WhitelistState` always adds the defaults before reading the user file (`lib/core/base.ps1:600-604`). The tree does not show how a user removes a default. Left unresolved.
- Steam `userdata` screenshots (`lib/clean/apps.ps1:324`) sit under `C:\Program Files (x86)`, a protected prefix (`lib/core/base.ps1:184-195`), so that row is blocked by the guard (inferred).
- `NuGet\packages` is deliberately not cleaned (`lib/clean/dev.ps1:293-295`); Docker build cache is review-only (`lib/clean/dev.ps1:436-441`); the whole JVM ecosystem removes nothing (`lib/clean/dev.ps1:711-715`); Mix archives are kept (`lib/clean/dev.ps1:476-482`).

### 3.2 Absent on purpose

These common Windows reclaim targets do not exist in this CLI.

| Target | Status | Evidence |
|---|---|---|
| Windows Update cache (`SoftwareDistribution`) | Absent. Anything under `C:\Windows` is refused outright by `Test-SafePath` | `lib/clean/system.ps1:21-22`, `lib/core/base.ps1:184-195`, `SYNC.md:86`, `.claude/rules/clean.md:26` |
| Recycle Bin emptying | Removed (`Clear-RecycleBin`) | `SYNC.md:82` |
| Disk Cleanup tool, component store (`DISM /StartComponentCleanup`) | Removed (`Invoke-DiskCleanupTool`, `Invoke-ComponentStoreCleanup`) | `SYNC.md:82` |
| Old Downloads cleanup | Removed (`Clear-OldDownloads`); Analyze only reports "Old Downloads (90d+)" | `SYNC.md:82`, `cmd/analyze/overview.go:69`, `cmd/analyze/overview.go:93-141` |
| Prefetch | Removed and protected by the Windows root rule | `SYNC.md:145` |
| `C:\Windows\SystemTemp`, `C:\Windows\Logs` | Out of reach | `SYNC.md:110`, `SYNC.md:86` |
| Thumbnail and icon caches, Explorer cache | Explorer folder is a default whitelist root; no row clears thumbnails | `lib/core/base.ps1:168` |
| Winsock, Search index, font cache, Store cache | Deliberately not touched | `.claude/rules/optimize.md:17` |
| `Windows.old`, hibernation file, system restore points | No code references them (grep of `lib` and `bin` found nothing) | grep, `lib/clean/*.ps1`, `bin/*.ps1` |

### 3.3 Catalog of what does run

| Category | Representative paths | Needs admin | Full list at |
|---|---|---|---|
| User temp | `%TEMP%` top-level files, 7 days; `%LOCALAPPDATA%\Temp\*.log`, `%APPDATA%\*.log` | no | `lib/clean/user.ps1:21-34`, `lib/clean/user.ps1:104-130` |
| Crash and error reports | `%LOCALAPPDATA%\Microsoft\Windows\WER\ReportArchive`, `ReportQueue`, `%LOCALAPPDATA%\CrashDumps` (`.dmp` only in the user pass) | no | `lib/clean/user.ps1:66-98` |
| Browser caches | `%LOCALAPPDATA%\Google\Chrome\User Data\<profile>\Cache`, `Code Cache`, `GPUCache`, root `ShaderCache`, `GrShaderCache`; Edge, Brave, Vivaldi, Firefox `cache2` | no | `lib/clean/caches.ps1:33-80` |
| App caches (Electron/CEF) | `%APPDATA%\discord\Cache`, `Slack\Cache`, `Code\CachedData`, `Cursor\Cache`, new Teams `...\EBWebView\<profile>\Cache` | no | `lib/clean/caches.ps1:100-213` |
| GPU shader caches (opt-in) | `%LOCALAPPDATA%\NVIDIA\DXCache`, `AMD\DXCache`, `D3DSCache`, `VulkanCache` | no | `lib/clean/caches.ps1:236-287` |
| Developer caches | npm, pnpm, Yarn Classic `%LOCALAPPDATA%\Yarn\Cache`, Bun, pip, Poetry, Go build and module cache, NuGet `v3-cache`, `~\.cargo\registry\cache`, `~\.hex\cache`, JetBrains, Visual Studio `ComponentModelCache`, `ImageCache` | no | `lib/clean/dev.ps1:180-678` |
| Office and sync clients | `%LOCALAPPDATA%\Microsoft\Office\16.0\Wef`, Outlook offline address books, OneDrive `setup\logs`, Google `DriveFS\Logs` | no | `lib/clean/apps.ps1:22-99` |
| Creative suites | `%APPDATA%\Adobe\Common\Media Cache Files`, `Peak Files`, `%LOCALAPPDATA%\Adobe\*\Cache`, `Autodesk\*\Cache` | no | `lib/clean/apps.ps1:101-150` |
| Game launchers | `%LOCALAPPDATA%\Steam\htmlcache`, Epic `webcache`, EA app `BrowserCache`, GOG `%PROGRAMDATA%\GOG.com\Galaxy\webcache`, Ubisoft `cache\http2`, Battle.net `Cache`, week-old launcher logs | no (ProgramData is writable for GOG data the way the code reads it) | `lib/clean/apps.ps1:156-218` |
| Game media (opt-in) | `%USERPROFILE%\Videos\NVIDIA`, `Videos\OBS`, `Videos\Captures`, `Pictures\Steam Screenshots`, 90 days | no | `lib/clean/apps.ps1:224-442` |
| System logs and dumps | `%PROGRAMDATA%\Microsoft\Windows\WER\ReportQueue`, `ReportArchive`, `%TEMP%\MSI*.LOG` at 30 days | yes (`--system`) | `lib/clean/system.ps1:23-65` |
| Vendor installer residue | `C:\NVIDIA\DisplayDriver\<version>` | yes (`--system`) | `lib/clean/system.ps1:119-159` |
| Delivery Optimization | Service-owned cache, asked through CIM `MSFT_DeliveryOptimizationFile.Delete` | yes (`--system`) | `lib/clean/system.ps1:165-312` |

### 3.4 Process and lock guard

`Clear-MoleAppCache` runs the idle check again after sizing and before deletion. It passes only when no process has one of the owner names and the lockfile can be opened with no sharing (`lib/clean/cache_guard.ps1:6-37`). A preview lists the item only if the owner is idle at the end of sizing (`lib/core/file_ops.ps1:739-742`, `lib/core/file_ops.ps1:781-789`).

### 3.5 Path protection

- Protected prefixes and exact paths. `C:\Windows`, `System32`, `SysWOW64`, `Program Files`, `Program Files (x86)`, Defender folders, `%SYSTEMROOT%`, `%WINDIR%`; the profile root exactly (`lib/core/base.ps1:184-199`).
- Segments refused everywhere. `Microsoft\Credentials`, `Protect`, `Crypto`, `Vault`, `SystemCertificates`, `Windows\Start Menu` (`lib/core/base.ps1:205-212`).
- Segments refused during `clean` only. `.ssh`, `.gnupg`, `.aws`, `.azure`, `.kube`, `.docker`, `Login Data`, `Cookies`, `Web Data`, `Local Storage`, `Session Storage`, `IndexedDB`, `Sessions`, `Local Extension Settings`, `Sync Data`, `Service Worker\CacheStorage`, Adobe Team Projects Cache, Poetry virtualenvs, Yarn Berry caches, VS Code `User\History`, `.claude` and `.codex` transcripts (`lib/core/base.ps1:219-248`).
- UWP data under `%LOCALAPPDATA%\Packages` is refused, with one exception for the new Teams WebView `Cache`, `Code Cache`, `GPUCache` during `clean` (`lib/core/base.ps1:489-507`).
- Existing paths resolve to their final spelling, so a junction into a protected tree, an 8.3 name or a trailing-dot alias is refused; a path that exists but cannot be resolved counts as protected (`lib/core/base.ps1:346-429`, `lib/core/base.ps1:540-583`, `AGENTS.md:89`).

### 3.6 Analyze protection (separate Go list)

Refused with everything below: `\Windows`, `\Program Files`, `\Program Files (x86)`, `\ProgramData`, `\Users\Default`, `\Users\Public`, `\Recovery`, `\System Volume Information`, `\$Recycle.Bin`, `\Config.Msi`, `\PerfLogs`, `\Boot`, `\Documents and Settings`, `\$WinREAgent` (`cmd/analyze/protect_windows.go:13-28`). Also paging files at any volume root, everything at the system drive's first level, `\Users` and each profile root, the env-var folders, Docker Desktop state, and the user head folders (Desktop, Documents, Downloads, Pictures, Videos, Music, AppData, AppData Local, Roaming, LocalLow, a OneDrive root, Mole's own folders) (`cmd/analyze/protect_windows.go:30-167`). The head folders themselves are refused but their contents can be deleted (`cmd/analyze/protect_windows.go:122-124`, `SYNC.md:50`).

## 4. Uninstall on Windows

### 4.1 Discovery

| Source | How | Source |
|---|---|---|
| Registry Uninstall keys | `HKLM` and `HKCU`, each in the native view and `WOW6432Node` (4 roots) | `bin/uninstall.ps1:895-900` |
| Store / MSIX | `Get-AppxPackage` filtered by `Test-ListableAppxPackage`, display name from the manifest | `bin/uninstall.ps1:514-534`, `bin/uninstall.ps1:939-981` |
| winget, Scoop, Chocolatey | None. No discovery code references them (grep of `bin/uninstall.ps1`). They matter only for Mole's own install (`bin/remove.ps1:44-64`, `bin/update.ps1:197-206`) | grep, `bin/uninstall.ps1` |
| Portable apps | Not discovered | n/a |

Entry filters (an entry is hidden when any holds). No `DisplayName`, no uninstall string, `SystemComponent` flag, `NoRemove` flag, an update entry (`ParentKeyName`, `ReleaseType` Update or Hotfix, `KB######` default value), or a name on the protected list (`bin/uninstall.ps1:409-416`, `bin/uninstall.ps1:330-347`).

Store filters. Not a framework, resource package or bundle; `SignatureKind` is not System; name not `Microsoft.Windows.*`; not on the protected package list; install location not under `%SystemRoot%`; manifest readable and `AllowExternalContent` absent or false/0 (sparse packages are shell integration of a desktop app) (`bin/uninstall.ps1:514-534`, `bin/uninstall.ps1:557-567`).

Merging. Duplicate registrations merge when they share install folder and name, the same uninstall command, or name plus version plus publisher; two different install folders never merge (`bin/uninstall.ps1:456-510`).

Size. Registry `EstimatedSize` (KB). A value above 10 GB is measured against the install folder and the measurement wins when it is at least 10 times smaller (`bin/uninstall.ps1:49-50`, `bin/uninstall.ps1:370-390`). Store size is a bounded 15 s read-only walk (`bin/uninstall.ps1:838-862`).

### 4.2 Protection lists

- Protected app names (wildcards). `Microsoft Windows`, `Windows Feature Experience Pack`, `Microsoft Edge`, `Microsoft Edge WebView2`, `Windows Security`, `Microsoft Visual C++ *`, `Microsoft .NET *`, `.NET Desktop Runtime*`, `Microsoft Update Health Tools`, `NVIDIA Graphics Driver*`, `AMD Software*`, `Intel*Driver*` (`bin/uninstall.ps1:82-95`).
- Protected Store packages. `Microsoft.WindowsStore`, `StorePurchaseApp`, `DesktopAppInstaller`, `SecHealthUI`, `GetHelp`, `XboxGamingOverlay` (`bin/uninstall.ps1:110-117`).

### 4.3 Leftover matching

Windows has no leftover search. After the uninstall entry disappears, Mole considers exactly one folder, the registered `InstallLocation`, and recycles it only if all of these hold (`bin/uninstall.ps1:307-328`).

1. `Test-MoleInstallFolder` accepts it. Rejected are drive roots, the profile and its shell folders, `Program Files`, `ProgramData`, `AppData`, `Windows`, shared names such as `Common Files`, `Installer`, `Temp`, `WindowsApps`, any Steam library root, `steamapps` and anything in it outside one game's `steamapps\common\<Game>` (`bin/uninstall.ps1:192-267`).
2. `Test-MoleInstallContentFree` finds no user content. The root and immediate children are checked completely for folders named `roms`, `saves`, `savegames`, `states`, `bios`, `games`, `gog games`, `steamapps`, `.egstore`, for `goggame-*.info`, `galaxy-2.0.db`, `libraryfolder.vdf`, and for virtual disks `.vhdx`, `.vhd`, `.vmdk`, `.vdi`. A link, more than 4096 entries or more than 2 s keeps the folder (`bin/uninstall.ps1:147-187`).
3. The uninstall entry is gone (`bin/uninstall.ps1:813-829`).
4. No other registered owner (HKLM and HKCU keys, Appx packages) has the same, parent or child path; an incomplete owner inventory keeps the folder (`bin/uninstall.ps1:271-326`).

It does not touch `%APPDATA%`, `%LOCALAPPDATA%`, registry keys left behind, services, or scheduled tasks (`SYNC.md:181`, `SYNC.md:204`, `SYNC.md:103`). The CLI never ports Mole for Windows' measured per-app leftover catalog (`AGENTS.md:33`).

### 4.4 Elevation

Mole starts the uninstaller with `Start-Process -Wait` and does not request elevation. The vendor installer raises UAC itself (`AGENTS.md:124`, `bin/uninstall.ps1:785-787`). The wait ceiling is 30 minutes (`bin/uninstall.ps1:775-811`).

## 5. Optimize, maintenance and status metrics

### 5.1 Optimize tasks

Section 2.4 lists the tasks. Order in a real run: system health line (CIM `Win32_OperatingSystem`, `Win32_LogicalDisk`, `Win32_Processor`), disk health, disk optimize, DNS, ARP, startup count, Windows Update, system image, optional SFC (`bin/optimize.ps1:69-99`, `bin/optimize.ps1:767-796`). Maintenance state changes are logged `OPTIMIZED` or `FAILED` (`bin/optimize.ps1:240`, `bin/optimize.ps1:295`, `bin/optimize.ps1:331`).

### 5.2 Status metrics collection

The Go binary calls Win32 and gopsutil directly. It does not use WMI or PowerShell. gopsutil v3.24.5 pulls `go-ole` and `wmi` as indirect modules (`go.mod:11`, `go.mod:21`, `go.mod:36`), but a grep of `cmd/status` finds no direct import of either, and no `os/exec` use.

| Metric | API | Source |
|---|---|---|
| Platform | `RtlGetVersion` | `cmd/status/metrics_windows.go:35-38` |
| Uptime | `GetTickCount64` | `cmd/status/metrics_windows.go:40-43` |
| CPU usage, per core, counts | gopsutil `cpu.Percent`, `cpu.Counts`; 100 ms minimum sample window | `cmd/status/metrics_windows.go:53-119` |
| Memory, swap, cache | gopsutil `mem.VirtualMemory`, `mem.SwapDevices`, `GetPerformanceInfo` for system cache; commit charge is not called swap | `cmd/status/metrics_windows.go:123-171` |
| Disks | `GetLogicalDrives`, `GetDriveType` (fixed and removable only), `GetDiskFreeSpaceEx`, `GetVolumeInformation` | `cmd/status/metrics_windows.go:184-251` |
| External-drive flag | `DeviceIoControl` `IOCTL_STORAGE_QUERY_PROPERTY` bus type (USB, 1394, SD, MMC); zero access rights, no admin | `cmd/status/metrics_windows.go:253-294` |
| Disk IO | gopsutil `disk.IOCounters` | `cmd/status/metrics_windows.go:296-310` |
| Network counters and adapters | gopsutil `net.IOCounters`, `GetAdaptersAddresses` (skips down and loopback, flags tunnels) | `cmd/status/metrics_windows.go:312-387` |
| System proxy | Registry Internet Settings key | `cmd/status/metrics_windows.go:389-404` |
| Processes | `NtQuerySystemInformation(SystemProcessInformation)` | `cmd/status/metrics_windows.go:406-458` |
| Battery | `GetSystemPowerStatus` plus battery device IOCTLs for design and full capacity and cycles, via `CM_Get_Device_Interface_List` | `cmd/status/metrics_windows.go:30`, `cmd/status/metrics_windows.go:460-578` |
| Hardware | Registry `HARDWARE\DESCRIPTION\System\BIOS`, `CentralProcessor\0`, `Windows NT\CurrentVersion`; refresh rate via `EnumDisplaySettingsW` | `cmd/status/metrics_windows.go:580-651` |

Left out on Windows. Thermal sensors, GPU, Bluetooth, Trash and zombie processes (`cmd/status/metrics.go:53-55`). No unprivileged CPU temperature source exists, so the score has no thermal term (`cmd/status/metrics_health.go:52-54`).

Health score (`cmd/status/metrics_health.go:55-191`). Start at 100. Subtract up to 30 for CPU (above 50 percent), 25 for memory (above 70), 20 for disk used (above 80, first disk only), 10 for disk IO (above 50 MB/s combined). Flat subtractions for memory pressure (5 or 15), battery service state (5 or 2), uptime over 7 days (1) or 14 days (3). A failing SMART state caps the score at 44. Bands are Excellent from 85, Good from 65, Fair from 45, else "Needs Attention" (`cmd/status/metrics_health.go:46-49`, `cmd/status/metrics_health.go:174-184`). Constants are in `cmd/status/metrics_health.go:8-50`.

Preferences. `status_prefs`, a `key=value` file under `~\.config\mole`, written with a Windows lock helper (`cmd/status/prefs.go:1-6`, `cmd/status/prefs_lock_windows.go`).

## 6. Analyze and status TUIs

Same Go code or different? Windows has its own module, `github.com/tw93/mole/windows`, Go 1.24, Bubble Tea 1.3.10, lipgloss 1.1.0 (`go.mod:1-13`). Both programs split into platform-neutral files and `_windows.go` files, with `!windows` stubs that keep tests building and make destructive paths fail closed (`cmd/analyze/platform_other.go:12-13`, `cmd/analyze/platform_other.go:46`, `cmd/status/metrics_other.go:10-12`). The analyzer's non-Windows `main` prints "analyze is only supported on Windows" and exits 1 (`cmd/analyze/main_other.go:10-13`). SYNC.md shows fixes ported between this tree and the macOS CLI together with their tests, so the layout and key set follow the macOS programs (`SYNC.md:219-220`, `cmd/analyze/constants.go:18-19`, `README.md:173`). Whether the macOS tree is a fork of the same code is not checkable here.

Terminal handling.
- Launch. `analyze.ps1` and `status.ps1` run the `.exe` and pass its exit code on (`bin/analyze.ps1:77-86`, `bin/status.ps1:67-71`). `main_windows.go` uses `tea.WithAltScreen()` (`cmd/analyze/main_windows.go:38`); `status` does the same (`cmd/status/main.go:304`).
- Row widths use `ansi.TruncateLeft` and a `truncateMiddle` that keeps the tail; CJK width bugs were fixed with `TestTruncateMiddleNeverExceedsTheColumnBudget` (`SYNC.md:219`, `SYNC.md:89`).
- The PowerShell menus (uninstall, purge, installer) use `[Console]::ReadKey`, ANSI escapes written with `Write-Host`, and `Format-SizeListEntry` for display-cell widths because ConsoleHost's width helper counts CJK as one cell (`bin/uninstall.ps1:1013-1015`, `lib/core/ui.ps1:46-50`, `AGENTS.md:47`).
- The spinner uses a native timer and writes Unicode through `WriteConsoleW` on a real console, so it needs no code page change (`AGENTS.md:83`, `lib/core/log.ps1:298-300`).
- Interactive means ConsoleHost, output and input not redirected, `TERM` not `dumb` (`lib/core/log.ps1:290-296`). `Clear-Host` is skipped when output is redirected because PowerShell 7 throws (`lib/core/ui.ps1:36-44`).
- Opening files uses `ShellExecute` on a COM STA thread with a 10 s timeout, and "show in Explorer" starts `explorer.exe /select,"path"` (`cmd/analyze/platform_windows.go:163-215`, `cmd/analyze/constants.go:23`).
- Windows PowerShell 5.1 is the supported target; the same suite runs under pwsh 7 (`AGENTS.md:93`). Scripts must be ASCII or UTF-8 with BOM (`AGENTS.md:95`).

## 7. Deletion mechanics

Four separate sinks. Do not assume one safety property covers all four.

### 7.1 Permanent per-entry delete (clean, purge)

- Files go through `Remove-Item -LiteralPath -Force` (`lib/core/file_ops.ps1:662`, `lib/core/file_ops.ps1:768`). Directories go entry by entry, deepest path first, with `[IO.Directory]::Delete(path, $false)`, never recursive (`lib/core/file_ops.ps1:232-247`, `lib/core/file_ops.ps1:429`, `lib/core/file_ops.ps1:439-455`).
- A read-only attribute is cleared before the directory delete (`lib/core/file_ops.ps1:239-241`).
- The README says `clean` and `purge` delete permanently (`README.md:122`, `SECURITY_AUDIT.md:62`).
- Pre-delete walk. `Get-TreeItemsNoFollow` lists every entry without entering junctions or directory symlinks, checks each against `Test-SafePath`, and binds each to its volume serial and file index with `CreateFile` plus `GetFileInformationByHandle` (`lib/core/file_ops.ps1:32-91`, `lib/core/file_ops.ps1:197-229`, `lib/core/file_ops.ps1:313-367`). Any protected or whitelisted child refuses the whole directory. The walk has a 15 s budget (`lib/core/file_ops.ps1:206-214`).
- Identity is rechecked after sizing, after the owner callback, and before each entry (`lib/core/file_ops.ps1:625`, `lib/core/file_ops.ps1:441-445`, `lib/core/file_ops.ps1:745`).
- Reason for the entry-by-entry design. Windows PowerShell 5.1 `Remove-Item` on a junction with children stops at an "Are you sure?" prompt, and answering yes deletes the target's files (`lib/core/file_ops.ps1:160-163`).

### 7.2 Recycle through `ITransferSource.RecycleItem` (uninstall leftovers, installer)

`Move-MoleItemToRecycleBin` compiles a COM wrapper at runtime, runs on an STA thread, and calls `ITransferSource.RecycleItem` on the parent's handler with the Recycle Bin shell item as destination. It never calls `RemoveItem` and never falls back to another API (`lib/core/file_ops.ps1:473-565`, `lib/core/file_ops.ps1:485-487`, `lib/core/file_ops.ps1:512`). Only fixed local drives are accepted (`lib/core/file_ops.ps1:500-501`). It fails if the item still exists afterwards and calls `SHUpdateRecycleBinIcon` so the desktop icon turns Full (`lib/core/file_ops.ps1:514-517`, `SYNC.md:106`). Recycled bytes are not counted as freed (`lib/core/file_ops.ps1:639`, `lib/core/file_ops.ps1:667`, `.claude/rules/uninstall.md:21`).

### 7.3 Recycle through `SHFileOperationW` (analyze)

`FO_DELETE` with `FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_SILENT | FOF_NOERRORUI | FOF_WANTNUKEWARNING` (`cmd/analyze/platform_windows.go:225-232`, `cmd/analyze/platform_windows.go:295-299`). Only fixed drives (`cmd/analyze/platform_windows.go:333-346`). The shell may still ask to delete permanently an item too large for the bin; the Recycle Bin item count before and after (`SHQueryRecycleBinW`) tells recycled from deleted, and the outcome goes to the log as `REMOVED` when the bin did not grow (`cmd/analyze/platform_windows.go:262-271`, `cmd/analyze/platform_windows.go:320-328`, `AGENTS.md:81`). Only 64-bit Windows (`cmd/analyze/platform_windows.go:281-283`). A guard walks the target (10 s, 100,000 entries), refuses protected descendants and reparse directories, and rechecks identity of every ancestor immediately before the sink (`cmd/analyze/recycle_guard_windows.go:15-16`, `cmd/analyze/recycle_guard_windows.go:49-124`, `cmd/analyze/delete.go:107-130`). The target path is validated raw before `filepath.Abs` so `..` cannot be resolved away (`cmd/analyze/delete.go:111-121`, `cmd/analyze/delete.go:155-169`).

### 7.4 Owner commands and service calls

- npm, pnpm, pip, Go, mise, Bun are asked to clean their own cache through a child PowerShell with a timeout (5 s query, 120 s clean) and `taskkill /T /F` on timeout (`lib/clean/dev.ps1:25-95`, `lib/clean/dev.ps1:107-178`).
- Delivery Optimization is cleaned through CIM `Invoke-CimMethod ... Delete` and verified by relisting (`lib/clean/system.ps1:285-310`).

### 7.5 Privilege and elevation

No self-elevation. Admin-only steps check the current token and are skipped otherwise (`lib/core/base.ps1:258-266`, `bin/clean.ps1:268-279`, `bin/optimize.ps1:206-209`). Elevation does not bypass the protected-path list (`SECURITY_AUDIT.md:72`). Tests must not need a UAC prompt (`AGENTS.md:84`).

### 7.6 Long paths

No `LongPathsEnabled` check, no `\\?\` prefixing in the deletion code (fixed-string grep of `lib`, `bin`, `cmd` finds `\\?\` only in `lib/core/base.ps1:420-426` where `GetFinalPathNameByHandle` output is folded back, in `bin/purge.ps1:384-386` where the in-use probe adds the prefix, and in the analyzer's protection matching at `cmd/analyze/protect_windows.go:62-63`, `cmd/analyze/protect_windows.go:173`, `cmd/analyze/protect_windows.go:208-211`). So the clean sink relies on whatever the .NET and PowerShell host allow. The development notes record that realistic app paths can exceed Windows PowerShell 5.1 path limits in tests (`docs/development-pitfalls.md:47`). The code does not say how a path over 260 characters behaves in a real clean. Left unresolved; test on a machine before relying on it.

### 7.7 Junctions and reparse points

- Lists and removes the link itself, never walks into it (`lib/core/file_ops.ps1:160-169`, `lib/core/file_ops.ps1:326-335`, `lib/core/file_ops.ps1:223`).
- A link in any ancestor makes `Get-MolePathSnapshot` return null and the target is skipped. A leaf link is allowed only when the caller passes `-AllowLeafLink` (`lib/core/file_ops.ps1:77-78`, `lib/core/file_ops.ps1:606`).
- Whitelist roots that are junctions are matched in both spellings (`lib/core/base.ps1:645-659`).
- Analyze labels junctions and symlinks with `isLinkEntry` and does not follow name-surrogate reparse points (`cmd/analyze/platform_windows.go:33-52`).
- Profile discovery skips reparse-point profile folders (`lib/clean/cache_guard.ps1:61`).

### 7.8 OneDrive placeholders

- Analyze counts online-only placeholders as 0 bytes and never recalls data (`cmd/analyze/disk_size.go:4-38`, `AGENTS.md:99`).
- Analyze refuses a OneDrive root, `OneDrive` and `OneDrive - <org>` folders in the profile, and the `OneDrive`, `OneDriveConsumer`, `OneDriveCommercial` environment folders (`cmd/analyze/protect_windows.go:116-117`, `cmd/analyze/protect_windows.go:148-157`).
- Purge's in-use probe does not follow placeholders (`bin/purge.ps1:364`).
- Clean has no placeholder logic; it never targets synced folders. `Clear-OneDriveCache` removes only week-old files in `OneDrive\setup\logs` (`lib/clean/apps.ps1:56-71`).

## 8. macOS features with no Windows counterpart (from SYNC.md only)

- LaunchAgent-owned leftovers and Time Machine (`tmutil`) handling (`SYNC.md:40`).
- Spotlight rules, Spotlight orphan pruning, `lsof` for in-use checks and its non-ASCII spellings, Foundation's item-replacement folder (`SYNC.md:41`, `SYNC.md:68`, `SYNC.md:172`).
- `~/Library/CloudStorage` skipping in Analyze (`SYNC.md:42`). Windows reads recall attributes instead (`SYNC.md:42`).
- Apple Mail attachment copies (`SYNC.md:172`).
- Superseded AI CLI releases (Claude Code, cursor-agent, Copilot) a running session holds (`SYNC.md:173`).
- llama.cpp model store (`SYNC.md:174`).
- Gradle wrapper distribution cleanup with usage metadata (`SYNC.md:179`); Windows removes nothing in the JVM ecosystem (`lib/clean/dev.ps1:711-715`).
- Package checkouts missing HEAD (`SYNC.md:180`).
- Offering data the user kept at uninstall later in Clean (`SYNC.md:181`), and the uninstall configuration reference review (`CLI-SYNC-06`, `SYNC.md:204`).
- A daily temp-root prune, Large files and Xcode review sections, DerivedData, the startup whitelist banner (`SYNC.md:164`).
- SwiftPM scratch outputs (`SYNC.md:68`).
- Installer formats `.dmg`, `.pkg`, `.iso`, `.xip`, installer `.zip` (`SYNC.md:18`).
- Homebrew bounded command path, launchd-based Optimize service signals (`SYNC.md:131`, `SYNC.md:133`).
- SYNC.md records only the items above. The macOS inventory slice should confirm the rest of the "no counterpart" list.

Differences that are decisions, not gaps.
- Third-party app updates stay out of both CLIs (`SYNC.md:206`, `AGENTS.md:35`).
- Dry runs write no operations log on Windows, but the macOS CLI records a dry-run session (`SYNC.md:210`).
- Unknown command and bad `history` arguments exit 2 on Windows and 1 on macOS; `history --json` keys differ (`SYNC.md:217`).
- Process CPU is relative to one logical CPU in both CLIs (`SYNC.md:205`).
- There is also a third CLI on a `linux` branch (`README.md:1`, `SYNC.md:9`).
- Mole for Windows (GUI), not this CLI, owns startup management, Doctor, treemap, MFT Analyze, deeper Status, AI cleanup and the elevated batch protocol (`AGENTS.md:29-35`).

## 9. Porting notes for dustpan

1. Dustpan will be a native Rust binary, so most of this tree's complexity (runtime-compiled C# for handles, COM, CIM through a child shell) becomes direct Win32 calls. The Go analyzer and status code already show the exact calls: `GetFileInformationByHandle` identity, `GetFinalPathNameByHandle`, `SHFileOperationW`, `IFileOperation`-style recycle, `NtQuerySystemInformation`, `GetAdaptersAddresses`, `GetDiskFreeSpaceEx` (`cmd/analyze/platform_windows.go:136-161`, `cmd/analyze/platform_windows.go:280-329`, `cmd/status/metrics_windows.go:406-458`).
2. Keep the safety chain as the product's spine: resolve to final path, check protected prefixes and segments in both spellings, check whitelist, bind to file identity, recheck at the sink (`lib/core/file_ops.ps1:105-158`, `lib/core/file_ops.ps1:32-103`). Model it as a type that cannot reach the sink without passing the guard.
3. Use a table-driven target catalog (name, path template, owner processes, lock file, age rule, privilege). The Windows cleanup list in this tree is hand-written PowerShell per app (`lib/clean/caches.ps1:158-168` already shows the table shape for Electron apps).
4. Process-idle and lock-file checks matter on Windows because open handles block deletes and caches are live databases (`lib/clean/cache_guard.ps1:6-37`). Rust can query processes with `NtQuerySystemInformation` or `CreateToolhelp32Snapshot` and test the lock with an exclusive open.
5. Decide the recoverability policy on purpose. Mole deletes cache permanently, recycles leftovers and installers, and recycles Analyze deletions. A friendly TUI should say before each action which of the four sinks applies (section 7).
6. The biggest Windows reclaim targets (Windows Update cache, component store, Recycle Bin, Windows.old, hibernation file, thumbnails) are absent here by design (section 3.2). If dustpan wants them, they need elevation and a bright opt-in; Mole's own incident list shows the cost of elevated overreach (`docs/incidents.md:34`).
7. Elevation is the main design choice with no precedent in this tree. Mole stays unelevated and skips steps. A Rust binary can embed a manifest for `requireAdministrator` or relaunch itself with `ShellExecuteEx` `runas`. Decide whether the TUI relaunches elevated for the admin stages only.
8. Uninstall on Windows is "run the registered uninstaller, then recycle the install folder". Reuse the discovery rules (4 registry roots, Store via `Get-AppxPackage`-equivalent WinRT `PackageManager`, filters in 4.1, protection lists in 4.2) and the command planning (MSI product code, Inno and NSIS detection, rereads before launch) (`bin/uninstall.ps1:633-765`). winget, Scoop and Chocolatey installed apps already appear in the registry, so discovery does not need them.
9. Registry Uninstall keys carry `QuietUninstallString`, `WindowsInstaller`, `EstimatedSize` (a KB DWORD that reads negative above 2 GB), `SystemComponent`, `NoRemove`, `ParentKeyName` (`bin/uninstall.ps1:350-365`, `bin/uninstall.ps1:403-454`). Read them as `u32` and distrust sizes above 10 GB (`bin/uninstall.ps1:49-50`).
10. Terminal. Enable virtual terminal processing and UTF-8 output yourself; the Go TUIs rely on Bubble Tea for that. Count display cells, not characters, for CJK and wide glyphs (`AGENTS.md:47`). A Rust crate such as ratatui with crossterm handles the Windows console and ConPTY.
11. Paths. Add long-path handling the original lacks: use `\\?\` prefixes or the long-path manifest, and test paths over 260 characters (section 7.6).
12. Sizes. Count what a file occupies, not its length. Port the placeholder, sparse and compressed rules (`cmd/analyze/disk_size.go:20-38`) and refuse to open online-only files.
13. Measure `%TEMP%` recursively with care. Mole only deletes top-level old files in `%TEMP%` and never enters subfolders (`SYNC.md:87`), a conservative choice. A fuller temp cleanup needs the "in use" probe that purge already uses (`bin/purge.ps1:367-429`).
14. Keep the operations log line format so a future `history` view can read both Mole-compatible and dustpan files (`lib/core/log.ps1:55-93`).
15. Distribution. Mole ships x64 only; the tools are not built for ARM64 (`lib/core/tui_binaries.ps1:42-46`, `scripts/build-release.ps1:168-169`, `packaging/scoop/mole.json:6-11`). Dustpan can target `aarch64-pc-windows-msvc` from the start. Sign binaries; Mole signs scripts and executables (`.github/workflows/release-windows.yml:231`).
16. The router's flag-refusal lesson. Reject unknown flags with a non-zero exit (`mole.ps1:260-263`, `mole.ps1:330-335`). A single `clap` definition covers this.
17. Unresolved from this tree. The default-whitelist editability question (3.1), long-path behavior (7.6), `mo clean --whitelist` interaction on macOS, and every `n/s` cell in section 2.
