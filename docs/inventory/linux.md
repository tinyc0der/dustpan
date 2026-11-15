# Mole CLI (Linux) inventory

Source: tw93/Mole branch `linux`, SHA 4de579cc, `VERSION` 0.6.0 (VERSION:1; SYNC.md:77 says released as V0.6.0-linux 2026-11-10; SYNC.md:101-109 lists unreleased work after it). Tree: a temporary extract of `origin/linux` from `../mole`. All paths below are relative to that root. Read-only research; no script executed.

Shape: a Bash 4+ GNU-tools CLI (`mole`, `bin/*.sh`, `lib/**.sh`) plus three Go binaries built by `make build` (Makefile:7-10): `trash-go` (cmd/trash), `status-go` (cmd/status), `analyze-go` (cmd/analyze). Go module `github.com/tw93/mole` (cmd/trash/main.go:20). No Rust, no libraries shared with the macOS or Windows branches (SYNC.md:3, AGENTS.md:3).

"macOS equivalent" values come from SYNC.md and AGENTS.md references to `main`. Where the tree is silent I mark `(unverified)` and rely on background knowledge of Mole Mac.

---

## 1. Commands and global flags

Dispatcher: `mole` (279 lines); `mo` is a 3-line symlink-resolving wrapper (mo:1-3). No completion script exists anywhere in the tree (grep for `completion` hits only AGENTS.md:18, SYNC.md:40, docs/ledger.md:12, all saying it is left out).

Dispatch table: `mole:245-272`. Command registry for help: `MOLE_COMMANDS` (mole:91-103). Root is refused at entry (mole:6-9) and again per command via `mole_refuse_root` (lib/core/common.sh:14-19).

| Command | Entry | Status | Note |
|---|---|---|---|
| `mo` (no args) | mole:260-266, menu mole:180-233 | IMPLEMENTED | Interactive menu on a TTY, else prints help. Five entries (mole:149). Menu keys: Up/Down, Enter, 1-5, M More, V Version, Q Quit (mole:212-230, footer mole:174). |
| `mo clean` | mole:246 -> bin/clean.sh | IMPLEMENTED (distro coverage PARTIAL: pacman and apt only, no dnf/zypper; AGENTS.md:17) | |
| `mo uninstall` | mole:246 -> bin/uninstall.sh | IMPLEMENTED (PARTIAL coverage: no dnf/rpm path, see section 4) | |
| `mo optimize` / `mo optimise` | mole:246, mole:254-257 | IMPLEMENTED (mostly report-only, see section 5) | British alias `optimise`. |
| `mo analyze` / `mo analyse` | mole:246, mole:250-253 -> bin/analyze.sh:10 -> `bin/analyze-go` | IMPLEMENTED | Exits 1 with "analyze is not built. Run make build" when binary missing (bin/analyze.sh:6-9). |
| `mo status` | mole:246 -> bin/status.sh:10 -> `bin/status-go` | IMPLEMENTED | Same missing-binary guard (bin/status.sh:6-9). |
| `mo purge` | mole:246 -> bin/purge.sh | IMPLEMENTED | |
| `mo history` | mole:246 -> bin/history.sh | IMPLEMENTED | Added 0.3.0 (SYNC.md:58). SYNC.md:40 and docs/ledger.md:12 still say "history left out"; stale, superseded by SYNC.md:58. |
| `mo update` | mole:246 -> bin/update.sh | IMPLEMENTED | Refuses source checkout (bin/update.sh:37-40) and non-installed trees (bin/update.sh:42-45). |
| `mo remove` | mole:246 -> bin/remove.sh | IMPLEMENTED | Removes Mole itself only (bin/remove.sh:1-3). |
| `mo help`, `--help`, `-h` | mole:258, show_help mole:105-137 | IMPLEMENTED | |
| `mo version`, `--version`, `-V` | mole:259, show_version mole:37-60 | IMPLEMENTED | Prints version, OS (os-release read, never sourced, mole:39-51), arch, kernel, session (`XDG_CURRENT_DESKTOP`, `XDG_SESSION_TYPE`), free disk, shell, install source (Release/Nightly/Source/Manual, mole:64-74). |
| unknown | mole:267-271 | n/a | `Unknown command`, exit 1. |

STUB count: 0. No command prints "not supported". Absent entirely (not even stubs): `mo installer`, `mo touchid`, `mo completion` (grep hits for touchid/installer: none in bin, lib, mole, install.sh).

Counts: 9 working commands (clean, uninstall, optimize, analyze, status, purge, history, update, remove) + help + version + menu = 12 entry points, 2 aliases (analyse, optimise). 9/9 implemented, 0 partial by code, 0 stub; 3 (clean, uninstall, optimize) are partial by distro coverage.

### Global flags

| Flag | Where | Status |
|---|---|---|
| `--debug` | Stripped from args anywhere, sets `MO_DEBUG=1` (mole:239-243); starts a session log `$XDG_STATE_HOME/mole/mole_debug_session.log` (mole:78-89, called mole:247, 251, 255) with header free of user/machine names; path printed after run (mole:143-145). Each bash command also accepts it itself (e.g. bin/clean.sh:21). | IMPLEMENTED |
| `-h`, `--help` | mole:258; each command prints own usage | IMPLEMENTED |
| `-V`, `--version` | mole:259 | IMPLEMENTED |

### Per-command flags

| Command | Flags | Source | Status |
|---|---|---|---|
| clean | `--dry-run`/`-n`, `--whitelist`, `--gpu-shaders`, `--debug`, `-h` | bin/clean.sh:16-42 | IMPLEMENTED |
| uninstall | `[APP_NAME ...]`, `--dry-run`/`-n`, `--list`, `--with-data`, `--yes`/`-y`, `--debug`, `-h`; unknown `-*` exits 1 | bin/uninstall.sh:18-54 | IMPLEMENTED |
| optimize | `--dry-run`/`-n`, `--debug`, `-h` | bin/optimize.sh:20-41 | IMPLEMENTED |
| analyze | `[PATH]`, `--json`, `-h`; env `MO_ANALYZE_PATH` overrides PATH (cmd/analyze/main.go:87-92) | cmd/analyze/main.go:19-38 | IMPLEMENTED |
| status | `--json`, `--watch`, `--interval DUR`, `--proc-cpu-alerts` (default true), `--proc-cpu-threshold PCT` (default 100), `--proc-cpu-window DUR` (default 5m), `-h`. JSON is automatic when stdout is not a TTY (cmd/status/main.go:35-47, 394) | cmd/status/main.go:26-33, 328-346 | IMPLEMENTED |
| purge | `--dry-run`/`-n`, `--paths`, `--yes`/`-y`, `--debug`, `-h` | bin/purge.sh:18-42 | IMPLEMENTED |
| history | `--json`, `--limit N` / `--limit=N`, `--debug`, `-h`; invalid arg exits 1 | bin/history.sh:125-157 (limit check history_limit_ok) | IMPLEMENTED |
| update | `--force`/`-f`, `--nightly`, `-h` | bin/update.sh:12-33 | IMPLEMENTED |
| remove | `--yes`/`-y`, `--dry-run`/`-n`, `-h` | bin/remove.sh:13-29 | IMPLEMENTED |
| install.sh (not a `mo` command) | `--version V`, `--archive FILE`, `--sums FILE`, `--latest`, `--nightly`, `--commit SHA`, `--latest-commit` | install.sh:6-11, 58-106 | IMPLEMENTED |

Distinct command-specific flags: clean 3, uninstall 4, optimize 1, analyze 1, status 6, purge 3, history 2, update 2, remove 2 = 24, plus 3 globals = 27, all IMPLEMENTED, 0 stub. (`-n`, `-y`, `-f` are short aliases, not counted.)

Exit codes: unknown flag exits 1 everywhere, including the Go binaries, which override Go's default 2 (cmd/status/main.go:348-351, cmd/analyze/main.go:40-43). History exits 1 on bad args "Mac" style, differing from Windows's 2 (SYNC.md:58).

Not supported by design (AGENTS.md:19): GUI, paid tier, background agents, schedulers, notifications, package upgrades, `autoremove`, emptying whole package caches, kernel removal, CPU governor/EPP/turbo/`platform_profile`/sysctl/`drop_caches` tuning.

---

## 2. Feature table

Privilege legend: `user` = runs as the desktop user; `sudo` = one bounded distro command under sudo; Mole never deletes or moves arbitrary paths as root (SYNC.md:15, AGENTS.md:25).

Safety rules referenced: **R1** refuse root (common.sh:14-19). **R2** protected-path list (delete.sh:20-37). **R3** plain-home-item check: inside `$HOME`, no symlinked component, user-owned (delete.sh:42-71). **R4** running-owner block, unknown probe counts as running (clean.sh:94-137, 236-244). **R5** dry-run lists exactly what a real run does (sizes only eligible children, clean.sh:219-233). **R6** one `operations.log` row per change (log.sh:50-68). **R7** same-device only, `rm --one-file-system` (delete.sh:228-233, 244). **R8** allowlist only, unknown dirs never selected (rules.sh:21-23). **R9** package-manager lock/busy check (clean.sh:650, 733-737). **R10** GNU tools only, `LC_ALL=C` for parsing (AGENTS.md:33-34, base.sh:80).

### 2.1 Clean (LNX-CLEAN)

| ID | Command | Flags/options | Behavior | Privilege | Safety rule | macOS equiv | Status | Source |
|---|---|---|---|---|---|---|---|---|
| LNX-CLEAN-01 | clean | `--dry-run`, `-n` | Preview: same code path, sizes only what a real run removes, labels rows `dry` | user | R5 | same | implemented | bin/clean.sh:18, 57-58, 246-250 |
| LNX-CLEAN-02 | clean | (default) | Walk allowlist `section\|label\|root\|owners\|selector`; remove children of each root (never the root); group rows with same label; skip children under 8 KB total | user | R3 R4 R7 R8 | differs (Mac uses per-category functions in lib/clean/*.sh; Linux one rules table) | implemented | lib/clean/rules.sh:1-20, 37-95; bin/clean.sh:193-264, 233 |
| LNX-CLEAN-03 | clean | (automatic) | Owner check per rule: exact process name (`pgrep -x`, 15-byte comm handling), `cmd:<ERE>` (`pgrep -f`), `flatpak:<id>` (`flatpak ps`), `snap:<name>` (cgroup scan). Running or unknown skips the rule with a named note | user | R4 | same (Mac `file_ops.sh` running-owner check, SYNC.md:25) | implemented | bin/clean.sh:94-145; lib/core/base.sh:231-248, 329-355, 391-411 |
| LNX-CLEAN-04 | clean | `--whitelist` | Paginated multi-select to choose rules to keep; stored in `$XDG_CONFIG_HOME/mole/whitelist` as rule labels or `/`,`~` paths/globs; unreadable file (even dangling link) aborts clean | user | R8 | differs (labels as well as paths; SYNC.md:34) | implemented | bin/clean.sh:19, 892-896; lib/clean/whitelist.sh:1-60 |
| LNX-CLEAN-05 | clean | `--gpu-shaders` | Opt-in Mesa (`mesa_shader_cache`, `_db`), NVIDIA (`~/.cache/nvidia`), Steam `shadercache` (native, Debian `~/.steam/debian-installation`, Flatpak), Lutris shaders. Skipped without flag | user | R4 | Linux-only flag (aligned from Windows CLI-SYNC-15, SYNC.md:59) | implemented | lib/clean/rules.sh:158-174; bin/clean.sh:15, 319 |
| LNX-CLEAN-06 | clean | (default) | Browser top-level caches: Chromium, Chrome, Brave, Edge, Vivaldi, Firefox `mozilla/firefox`, Zen | user | R4 | differs (paths) | implemented | lib/clean/rules.sh:72-78 |
| LNX-CLEAN-07 | clean | (default) | Chromium-family profile caches: `GPUCache`, `Code Cache`, `Dawn*`, `GrShaderCache`, `GraphiteDawnCache` per profile (a folder holding `Preferences`); browser-level `ShaderCache`, `component_crx_cache`, `OptGuide*`, `Crashpad/completed`. Never Cookies, History, Sessions, Storage, IndexedDB, Service Worker | user | R3 R4 | same set as Mac, paths differ | implemented | lib/clean/rules.sh:196-238 |
| LNX-CLEAN-08 | clean | (default) | Electron app caches under `~/.config/<App>`: VS Code (+Insiders, OSS), VSCodium, Cursor, Obsidian, Discord, Slack, Signal, Typora, Heroic: `Cache`, `Code Cache`, `GPUCache`, `Dawn*`, `Crashpad/completed`; editors also `CachedData`, `CachedExtensionVSIXs`, `logs` | user | R4 (system Electron `electron[0-9]*` owner, rules.sh:32) | same list shape; Spotify not ported (offline music) | implemented | lib/clean/rules.sh:246-286 |
| LNX-CLEAN-09 | clean | (default) | Editor `.obsolete` extension folders for VS Code, Insiders, OSS, Cursor (selector `obsolete`) | user | R3 R4 | same (Mac clean_editor_obsolete_extensions) | implemented | lib/clean/rules.sh:277-285, 562-571 |
| LNX-CLEAN-10 | clean | (default) | Apps: Thunderbird `cache2` per profile, Telegram `cache`+`media_cache` per account, GNOME Software cache, WebKitGTK `WebKitCache`+`MediaCache` | user | R4 | partial overlap (Telegram, Thunderbird) | implemented | lib/clean/rules.sh:120-152 |
| LNX-CLEAN-11 | clean | (default) | Thumbnails `~/.cache/thumbnails` | user | R3 | Linux-only (freedesktop thumbnail cache) | implemented | lib/clean/rules.sh:81 |
| LNX-CLEAN-12 | clean | (default) | About 55 developer-tool caches (full list section 3.5) | user | R4 R8 | differs (SYNC.md:28: Linux paths; macOS-only apps omitted) | implemented | lib/clean/rules.sh:288-388 |
| LNX-CLEAN-13 | clean | (automatic) | `uv cache prune` instead of rm (hardlinked cache); skipped if uv absent or running; dry run says "would run" | user | R4 | same (Mac `clean_uv_cache`) | implemented | bin/clean.sh:276-312; rules.sh:337 |
| LNX-CLEAN-14 | clean | (automatic) | Old versions of Claude Code and Cursor Agent: keep the one the `~/.local/bin/<tool>` link resolves into plus newest other; skip if running process can't be told | user | R4 | same (Mac clean_dev_ai_agents) | implemented | lib/clean/rules.sh:382-387, 525-559; bin/clean.sh:203-208 |
| LNX-CLEAN-15 | clean | (automatic) | Flatpak per-app `~/.var/app/<id>/cache` (one rule per app, owner `flatpak:<id>`) with name narrowing (spotify, huggingface, lm-studio, ms-playwright, Cypress, puppeteer, uv, wine, winetricks, tracker3, ibus kept; JetBrains, Google, lutris, deno, bazel, webkitgtk, thunderbird, pypoetry, node narrowed) | user | R3 R4 | Linux-only | implemented | lib/clean/rules.sh:403-413; lib/core/delete.sh:127-214 |
| LNX-CLEAN-16 | clean | (automatic) | Snap per-app `~/snap/<name>/{common,<rev>}/.cache` (owner `snap:<name>`) | user | R4 | Linux-only | implemented | lib/clean/rules.sh:419-433 |
| LNX-CLEAN-17 | clean | (automatic) | Game launchers: Lutris installer downloads, Wine `.msi` downloads (`suffix:.msi`) | user | R4 | Linux-only | implemented | lib/clean/rules.sh:178-189 |
| LNX-CLEAN-18 | clean | (automatic) | AUR build caches `~/.cache/yay`, `~/.cache/paru` | user | R4 | Linux-only | implemented | lib/clean/rules.sh:89-90 |
| LNX-CLEAN-19 | clean | (automatic) | Empty the freedesktop Trash `$XDG_DATA_HOME/Trash/{files,info}`, removes `.trashinfo` too; honors whitelist label `Trash` | user | R3 R7 | differs (Mac empties via Finder/own staging) | implemented | bin/clean.sh:350-390; base.sh:76 |
| LNX-CLEAN-20 | clean | (automatic) | "Not cleaned" section: five largest unknown `~/.cache/*` folders over 100 MB, sizes only, never selected | user | R8 | Linux-only (unverified) | implemented | bin/clean.sh:394-422 (threshold 407, top 5 at 420) |
| LNX-CLEAN-21 | clean | (automatic) | `~/.cache` redirected (symlink, `XDG_CACHE_HOME` outside home or inside config/data/state home): all cache rules skipped once with a note | user | R3 | Linux-only | implemented | bin/clean.sh:150-165, 329-336 |
| LNX-CLEAN-22 | clean | (automatic) | Refuse run when shell cannot write `~/.cache` and `~/.local/share` (sandbox/read-only): exits 1, "Nothing was removed" | user | none | same (`refuse_cleanup_without_write_access`, SYNC.md:89) | implemented | bin/clean.sh:819-837, 854 |
| LNX-CLEAN-23 | clean | (automatic) | `paccache -rk2` (keep 2 versions) under sudo; dry run uses `paccache -dk2` and parses "N candidates ... saved: X"; skip when `/var/lib/pacman/db.lck` exists | sudo | R9, never `-rk0`/`-Scc` (AGENTS.md:25) | Linux-only | implemented | bin/clean.sh:647-682, 440-441 |
| LNX-CLEAN-24 | clean | (automatic) | `apt-get autoclean` under sudo, previewed via `apt-get -s autoclean` (counts `Del ... [size]`); never `apt-get clean`; skipped while apt/dpkg/aptitude/synaptic/unattended-upgrade runs or unknown | sudo | R9 (lib/core/base.sh:356-385) | Linux-only | implemented | bin/clean.sh:712-758, 445-446 |
| LNX-CLEAN-25 | clean | (automatic) | `journalctl --vacuum-time=2weeks` under sudo; before/after from `journalctl --disk-usage`; dry run reports current usage only | sudo | bounded retention floor | Linux-only | implemented | bin/clean.sh:434-438, 686-705 |
| LNX-CLEAN-26 | clean | (automatic) | `flatpak uninstall --user --unused --noninteractive` (timeout 900); logs before/after size of `$XDG_DATA_HOME/flatpak` | user | none beyond flatpak's pin logic | Linux-only | implemented | bin/clean.sh:522-544 |
| LNX-CLEAN-27 | clean | (automatic) | Report only: system Flatpak unused runtimes with command `sudo flatpak uninstall --system --unused` | user (report) | never run under sudo | Linux-only | implemented (report) | bin/clean.sh:550-556 |
| LNX-CLEAN-28 | clean | (automatic) | Report only: Snap disabled revisions (`snap list --all`, sizes from `.snap` files, threshold 1 MiB) with `sudo snap remove <name> --revision=<rev>` | user (report) | never removed | Linux-only | implemented (report) | bin/clean.sh:764-784 |
| LNX-CLEAN-29 | clean | (automatic) | Report only: coredumps (`coredumpctl list`, size of `/var/lib/systemd/coredump`, over 1 MiB) with `sudo rm` hint | user (report) | never removed | Linux-only | implemented (report) | bin/clean.sh:559-568 |
| LNX-CLEAN-30 | clean | (automatic) | Report only: Docker/Podman `system df` Reclaimable total over 100 MB with `<tool> system prune` hint; Docker only if daemon socket present; Podman only if `~/.local/share/containers` exists | user (report) | never pruned | differs (Mac has Docker row in dev.sh) | implemented (report) | bin/clean.sh:578-617 |
| LNX-CLEAN-31 | clean | (automatic) | btrfs/snapper honesty: pacman or apt bytes on a snapshotted subvolume are shown but excluded from the freed total ("freed as snapshots rotate") | user | n/a | Linux-only (macOS APFS snapshots are separate) | implemented | bin/clean.sh:660-669, 742-747; lib/core/snapshots.sh:1-58; SYNC.md:70 |
| LNX-CLEAN-32 | clean | (automatic) | Sudo gate: asked once before work on Mole's own `Password:` line; Enter continue, Space skip, ESC/q cancel; any other key = early password, keys drained; keepalive; `mole_sudo_end` drops only a timestamp Mole created; never prompts without a TTY | sudo | R1; typed chars never reach shell (SYNC.md:93) | same as Mac minus Touch ID (SYNC.md:31) | implemented | bin/clean.sh:475-518; lib/core/common.sh:29-76 |
| LNX-CLEAN-33 | clean | (automatic) | Summary block: removed/potential bytes + item count, free space before/after on `$HOME` | user | R6 | same | implemented | bin/clean.sh:862-887 |

### 2.2 Uninstall (LNX-UNIN)

| ID | Command | Flags/options | Behavior | Privilege | Safety rule | macOS equiv | Status | Source |
|---|---|---|---|---|---|---|---|---|
| LNX-UNIN-01 | uninstall | (no args, TTY) | Scan inventory, paginated multi-select (name, size, last used, source); returns to list after a run (Enter, 5 s idle exits) | user | needs TTY else exit 1 | same UI | implemented | bin/uninstall.sh:249-279, 988-1028 |
| LNX-UNIN-02 | uninstall | `APP_NAME ...` | Exact, case-insensitive match on app ID first, then display name; ambiguity refused, never guessed | user | exact match | differs (Mac matches app names) | implemented | bin/uninstall.sh:216-247 |
| LNX-UNIN-03 | uninstall | `--list` | Table on a TTY; JSON array when piped: `name, uninstall_name, source, kind, size, size_kb, last_used, last_used_epoch` | user | read-only | same (Mac JSON when piped, SYNC.md:36) | implemented | bin/uninstall.sh:186-212 |
| LNX-UNIN-04 | uninstall | `--dry-run` | Prints plan (leftovers, units, package extras) without running anything | user | R5 | same | implemented | bin/uninstall.sh:903-908 |
| LNX-UNIN-05 | uninstall | `--with-data` | Preselect settings and data leftovers; browser/mail profiles, offline music, model stores, game libraries, VMs, local-only chat stores stay unselected | user | holdback list | same concept | implemented | bin/uninstall.sh:343-349; lib/uninstall/inventory.sh:796-870 |
| LNX-UNIN-06 | uninstall | `--yes`, `-y` | Skip Mole's confirmation (required without TTY). Snap with data in `~/snap/<name>` still refuses `--yes`; Flatpak prompts itself and answers "n" off-TTY | user | snap data guard | same flag (unverified on Mac) | implemented | bin/uninstall.sh:662-665, 922-929; docs/ledger.md:68 |
| LNX-UNIN-07 | uninstall | (automatic) | Pacman/AUR removal: `sudo pacman -Rns` (pacman's own prompt, no `--noconfirm`); plan prints extra dependencies from `pacman -Rsp`; skip if `db.lck` | sudo | R9 | Linux-only | implemented | bin/uninstall.sh:424-428, 613-625 |
| LNX-UNIN-08 | uninstall | (automatic) | apt removal: `sudo apt-get remove` (never purge/autoremove, no `-y`); plan names dependents from `apt-get -s remove`; skipped while apt busy/unknown | sudo | R9 | Linux-only | implemented | bin/uninstall.sh:429-435, 626-642 |
| LNX-UNIN-09 | uninstall | (automatic) | Snap removal: `sudo snap remove`; Mole's confirmation is last because snap has none; warns snap deletes `~/snap/<name>` after a 31-day snapshot (`snap get system snapshots.automatic.retention`) | sudo | snap data guard | Linux-only | implemented | bin/uninstall.sh:436-440, 478-506, 658-680 |
| LNX-UNIN-10 | uninstall | (automatic) | Flatpak removal per installation (`--user`, `--system`, `--installation=<name>`); system/custom through sudo; data in `~/.var/app/<id>` kept (listed as orphan) | user or sudo | R9 | Linux-only | implemented | bin/uninstall.sh:681-706; inventory.sh:393-410 |
| LNX-UNIN-11 | uninstall | (automatic) | Omarchy web-app and TUI launchers, AppImages with a `~/.local/share/applications` launcher: launcher + file + icon to Trash; counted removed only once the first file left | user | R3 via Trash | Linux-only | implemented | bin/uninstall.sh:707-726; inventory.sh:435-502 |
| LNX-UNIN-12 | uninstall | (automatic) | Flatpak orphans (`~/.var/app/<id>` with no installed app in any installation) listed as removable rows; held-back ones unselected | user | R3 | Linux-only | implemented | inventory.sh:506-520; bin/uninstall.sh:326-333 |
| LNX-UNIN-13 | uninstall | (automatic) | Exact-ID leftovers to Trash, cache checked / data unchecked (details section 4.4) | user | exact IDs only | same principle (Mac bundle ID) | implemented | inventory.sh:726-790; bin/uninstall.sh:317-355 |
| LNX-UNIN-14 | uninstall | (automatic) | User systemd units: `systemctl --user disable --now` before trashing unit files or before package removal; no user bus leaves them in place; restored if the package stays | user | fail closed | Linux-only | implemented | bin/uninstall.sh:524-602, 772-806 |
| LNX-UNIN-15 | uninstall | (automatic) | Running check: exe/argv/comm for packages, `flatpak ps`, snap cgroup, AppImage exe; unknown probe blocks | user | R4 | same concept | implemented | bin/uninstall.sh:444-474; base.sh:282-326, 413 |
| LNX-UNIN-16 | uninstall | (automatic) | After removal: `update-desktop-database` and `gtk-update-icon-cache` rebuilds; stale `mimeapps.list` associations reported, never rewritten | user | report only | Linux-only | implemented | bin/uninstall.sh:811-867 |
| LNX-UNIN-17 | uninstall | (automatic) | "Last used" estimate from newest mtime of the app's config/data/state/cache plus program atime when later than ctime+60s | user | n/a | differs (Mac uses LaunchServices/Spotlight, unverified) | implemented | bin/uninstall.sh:81-130 |
| LNX-UNIN-18 | uninstall | (automatic) | Protected packages never listed (base, linux, systemd, pacman, apt, dpkg, snapd, session/DE, drivers, Omarchy/Hyprland stack, login-shell package) | n/a | protection | same concept (Mac SIP/system apps) | implemented | inventory.sh:141-158 |

### 2.3 Optimize (LNX-OPT)

Outcomes per task: applied, unchanged, skipped, unavailable, attention, failed (lib/optimize/tasks.sh:4-10). Every task records exactly one (bin/optimize.sh:121-125). Nothing uses sudo (tasks.sh:11, AGENTS.md:19).

| ID | Command | Flags/options | Behavior | Privilege | Safety rule | macOS equiv | Status | Source |
|---|---|---|---|---|---|---|---|---|
| LNX-OPT-01 | optimize | `--dry-run` | Run all tasks in preview; summary "Would apply N" | user | R5 | same | implemented | bin/optimize.sh:22, 146-151 |
| LNX-OPT-02 | optimize | (header) | System line `used/total GB RAM \| used/total GB Disk \| Uptime Nd` from `/proc/meminfo`, `df -kP /`, `/proc/uptime` | user | read-only | same (Mac ⚙ System line) | implemented | bin/optimize.sh:47-57 |
| LNX-OPT-03 | optimize | (header) | Performance diagnosis from PSI `/proc/pressure/{cpu,memory,io}` avg60 thresholds 25/10/25; names top 3 processes via `top -b -n 2` with a throwaway HOME | user | read-only | Linux-only | implemented | bin/optimize.sh:59-107 |
| LNX-OPT-04 | optimize | | Font Cache Refresh: `fc-cache` | user | none | differs (Mac has no fc) | implemented (applies) | lib/optimize/tasks.sh:98-115 |
| LNX-OPT-05 | optimize | | App Launchers: `update-desktop-database -q ~/.local/share/applications`; reports `.desktop` files whose program is gone | user | report broken | Linux-only | implemented (applies + report) | tasks.sh:117-230 |
| LNX-OPT-06 | optimize | | Icon Cache Refresh: `gtk-update-icon-cache -q -f` on user icon themes | user | none | Linux-only | implemented (applies) | tasks.sh:320-358 |
| LNX-OPT-07 | optimize | | Autostart Entries: report `~/.config/autostart` and Hyprland start commands whose program is gone | user | report only | differs (Mac login items) | implemented (report) | tasks.sh:231-297 |
| LNX-OPT-08 | optimize | | Failed Services: `systemctl [--user] --failed`; hints `systemctl status <unit>` | user | report only | Linux-only | implemented (report) | tasks.sh:360-398 |
| LNX-OPT-09 | optimize | pacman only | Orphan Packages: `pacman -Qtdq`; hint `sudo pacman -Rns` | user | report only | Linux-only | implemented (report) | tasks.sh:401-424 |
| LNX-OPT-10 | optimize | pacman only | Package Database Lock: stale `db.lck` with no pacman/yay/paru/pamac/packagekitd/makepkg running; hint `sudo rm` | user | report only | Linux-only | implemented (report) | tasks.sh:440-461 |
| LNX-OPT-11 | optimize | pacman only | Pending Config Merges: `.pacnew .pacsave .pacorig` under `/etc`; hint `sudo pacdiff` | user | report only | Linux-only | implemented (report) | tasks.sh:739-803 |
| LNX-OPT-12 | optimize | pacman only | Packages To Rebuild (rebuild-detector approach, AUR against updated libs) | user | report only | Linux-only | implemented (report) | tasks.sh:932, 1071 |
| LNX-OPT-13 | optimize | pacman only | Unowned Files (pacreport/lostfiles approach, exclusions) | user | report only | Linux-only | implemented (report) | tasks.sh:1161, 1217 |
| LNX-OPT-14 | optimize | apt only | Unneeded Packages from `apt-get -s autoremove` | user | report only | Linux-only | implemented (report) | tasks.sh:810-836 |
| LNX-OPT-15 | optimize | apt only | Leftover Package Configs (`rc` dpkg state) | user | report only | Linux-only | implemented (report) | tasks.sh:838-901 |
| LNX-OPT-16 | optimize | apt only | Pending Config Merges: `.dpkg-dist/-new/-old`, `.ucf-*` | user | report only | Linux-only | implemented (report) | tasks.sh:746-749, 791 |
| LNX-OPT-17 | optimize | | Browser Databases: SQLite `VACUUM` of Chromium family (History, Favicons, Web Data, Top Sites, Shortcuts) and Gecko `*.sqlite` except `cookies.sqlite` (Firefox, Zen, Thunderbird) after read-only `quick_check`, browser closed; Cookies and Login Data never opened | user | integrity gate, owner running skips | differs (Mac VACUUMs Safari/Chrome DBs, unverified) | implemented (applies) | tasks.sh:470-635 (VACUUM 595, quick_check 525) |
| LNX-OPT-18 | optimize | | Disk TRIM: reads `findmnt` source/options and `fstrim.timer`; hint `sudo systemctl enable --now fstrim.timer` | user | report only | Linux-only | implemented (report) | tasks.sh:635-682 |
| LNX-OPT-19 | optimize | | Power Management: detects enabled `tlp`, `power-profiles-daemon`, `auto-cpufreq`, `tuned` conflicts | user | report only, never tunes | Linux-only | implemented (report) | tasks.sh:684-737 |
| LNX-OPT-20 | optimize | | Home Folder Ownership: root-owned files in `$HOME` | user | report only | Linux-only | implemented (report) | tasks.sh:298-318 |
| LNX-OPT-21 | optimize | | Broken Links: dangling symlinks in home | user | report only | Linux-only | implemented (report) | tasks.sh:903-1070 |
| LNX-OPT-22 | optimize | | Task order and package-manager gating | n/a | | n/a | implemented | bin/optimize.sh:175-196 |

Only four tasks change anything (fonts, launchers index, icon cache, browser DB vacuum). The rest print the command for the user. `optimize` does not flush DNS, rebuild Spotlight, reset Launch Services or kill daemons as Mole Mac does (from background knowledge, unverified here).

### 2.4 Analyze (LNX-ANA)

| ID | Command | Flags/options | Behavior | Privilege | Safety rule | macOS equiv | Status | Source |
|---|---|---|---|---|---|---|---|---|
| LNX-ANA-01 | analyze | none | Overview: machine-wide, rows anchored at `/`; Bubble Tea alt-screen TUI | user | read-only | same (forked from main at branch point, AGENTS.md:3) | implemented | cmd/analyze/main.go:87-104, 118 |
| LNX-ANA-02 | analyze | `[PATH]`, `MO_ANALYZE_PATH` | Scan one directory | user | | same | implemented | cmd/analyze/main.go:87-104 |
| LNX-ANA-03 | analyze | `--json` | Machine-readable output instead of TUI | user | | same (unverified) | implemented | cmd/analyze/main.go:20, 76-77; cmd/analyze/json.go |
| LNX-ANA-04 | analyze | keys | Up/k, Down/j, Enter/Right/l open, Left/h/b back, `r` rescan, `t` top files toggle, `/` filter, `s` live sort while scanning, `Space` multi-select, `o` open (max 20), `f` reveal (max 20; opens parent dir), `p` preview via xdg-open, Delete/Backspace move to Trash, `q`/Esc quit | user | batch caps | same keys | implemented | cmd/analyze/update.go:644-1046 (741, 760, 781, 786, 793, 840, 858, 871, 880, 926, 972, 991, 1046) |
| LNX-ANA-05 | analyze | (delete) | Delete means move to freedesktop Trash through `xdgtrash.Move`; protected trees refused: `/bin /boot /dev /etc /lib /lib64 /proc /root /run /sbin /sys /usr /nix/store /snap /var/lib /var/log`, exact `/` and `/var`, `~/.cache/doc`, `~/.local/share/keyrings`, `~/.gnupg`, `~/.ssh` | user | cross-device refused | differs (Mac uses Finder trash) | implemented | cmd/analyze/delete.go:116-173, 135 |
| LNX-ANA-06 | analyze | (overview) | Cleanable insights rows: Trash, pacman cache, yay, paru, npm, pip, uv, Go build, Go modules, Cargo registry, Rust toolchains, mise, Gradle, JetBrains, rootless Docker, Ollama, Hugging Face, Flatpak (user), Steam, Podman; system rows if readable: Flatpak (system), systemd Journal, Docker, Core Dumps | user | read-only | differs (Mac list) | implemented | cmd/analyze/insights.go:60-117 |
| LNX-ANA-07 | analyze | (scan) | Volumes from `/proc/self/mountinfo`, read once per scan with one `EvalSymlinks` of the root; enters local `/dev/` block mounts and same-filesystem btrfs subvolumes; network, FUSE, 9p, overlay, tmpfs, squashfs show "mounted, not scanned"; `du` excludes `.snapshots`; snapshots counted not scanned | user | fail closed | Linux-only | implemented | AGENTS.md:39; cmd/analyze/mounts.go:13-179; volumes.go:9-132 |
| LNX-ANA-08 | analyze | (scan) | Hardlinks keyed `(dev,ino)` counted once; noatime-aware "unused" label (later of atime, mtime) | user | | Linux-only | implemented | AGENTS.md:39; SYNC.md:38 |
| LNX-ANA-09 | analyze | (overview) | Old Downloads total (90 days) skips a foreign mount child | user | | same (main #1992) | implemented | cmd/analyze/insights.go:131-133; SYNC.md:106 |

### 2.5 Status (LNX-STAT)

| ID | Command | Flags/options | Behavior | Privilege | Safety rule | macOS equiv | Status | Source |
|---|---|---|---|---|---|---|---|---|
| LNX-STAT-01 | status | none | Live dashboard, 1 s refresh, slow-collector 30 s (cmd/status/main.go:17-21), health score, resize guidance when too short | user | read-only | same (fork) | implemented | cmd/status/main.go:109-159; SYNC.md:98 |
| LNX-STAT-02 | status | `--json` | One snapshot; automatic when stdout is not a TTY | user | | same | implemented | cmd/status/main.go:35-47, 394-397 |
| LNX-STAT-03 | status | `--watch`, `--interval` | NDJSON stream (default 1 s) | user | | same (unverified) | implemented | cmd/status/main.go:32-33, 310-323, 383-391 |
| LNX-STAT-04 | status | `--proc-cpu-*` | Persistent high-CPU process alerts | user | | same (unverified) | implemented | cmd/status/main.go:27-29, 99-107; process_watch.go |
| LNX-STAT-05 | status | keys | `q`/Esc/Ctrl-C quit, `k` toggle cat, `c` cycle CPU cores 2/4/8/all (persisted via prefs.go) | user | | same | implemented | cmd/status/main.go:116-129 |
| LNX-STAT-06 | status | (collectors) | Metric sources in section 5.2 | user | | differs | implemented | cmd/status/metrics_*.go |

### 2.6 Purge (LNX-PURGE)

| ID | Command | Flags/options | Behavior | Privilege | Safety rule | macOS equiv | Status | Source |
|---|---|---|---|---|---|---|---|---|
| LNX-PURGE-01 | purge | (default, TTY) | Scan roots, multi-select of build-output folders; rows older than 7 days preselected; recent or busy rows start unselected and have no select-all | user | recheck before delete (bin/purge.sh:232-250) | same | implemented | bin/purge.sh:134-250; lib/purge/scan.sh:25-28 |
| LNX-PURGE-02 | purge | `--dry-run` | Preview | user | R5 | same | implemented | bin/purge.sh:19, 293 |
| LNX-PURGE-03 | purge | `--paths` | Show/edit scan roots; config `$XDG_CONFIG_HOME/mole/purge_paths`; unreadable file exits 1 and keeps it | user | | same flag | implemented | bin/purge.sh:20, 90-132, 255-258; scan.sh:25 |
| LNX-PURGE-04 | purge | `--yes`, `-y` | Remove only rows older than 7 days without asking; needed outside a terminal | user | | same (unverified) | implemented | bin/purge.sh:21, 259-262, 293 |
| LNX-PURGE-05 | purge | (targets) | 34 name|evidence rows (section 3.6); target counts only with the project file beside it (stricter than Mac, SYNC.md:57); `build dist coverage vendor bin obj _build` only inside a git repo | user | evidence + git rules | stricter than Mac | implemented | lib/purge/scan.sh:33-75, 110-133, 160-176 |
| LNX-PURGE-06 | purge | (guards) | Authored-content probe (`.git` inside, keypairs, git-tracked files); node_modules lockfile + install-record rule; busy check via `/proc/<pid>` cwd, exe, fd, maps, args, `VIRTUAL_ENV`, fail-closed on unfinished listing; 7-day activity | user | fail closed | same + Linux `/proc` | implemented | scan.sh:263-311, 340-574; SYNC.md:57, 88 |
| LNX-PURGE-07 | purge | (sink) | `mole_remove_artifact`: rechecks device:inode identity, physical scan root, no symlinked parent, ownership, not a mount point, not protected; direct `rm -rf --one-file-system`, no Trash | user | R2 R7 | differs (Mac may trash) | implemented | lib/core/delete.sh:258-303 |
| LNX-PURGE-08 | purge | (roots) | Defaults: `~/www ~/dev ~/Dev ~/Projects ~/projects ~/GitHub ~/github ~/Code ~/code ~/Workspace ~/workspace ~/Repos ~/repos ~/src ~/Development`, `~/.codex/worktrees`, `~/.claude/worktrees`, plus home folders holding a project; skip `Desktop Documents Downloads Music Pictures Videos Public Templates go snap`; depth 6 | user | | same idea | implemented | scan.sh:74-88, 29 |
| LNX-PURGE-09 | purge | (not ported) | Cloud-storage roots, dataless folders (macOS only), `__pycache__` | n/a | | missing on Linux | not ported | SYNC.md:57 |

### 2.7 History, Update, Remove, Install

| ID | Command | Flags/options | Behavior | Privilege | Safety rule | macOS equiv | Status | Source |
|---|---|---|---|---|---|---|---|---|
| LNX-HIST-01 | history | (none), `--json`, `--limit N` | Group `operations.log` rows by `run=<id>` into sessions; JSON keys `logs, limit, sessions, operations`; buckets removed, trashed, skipped, failed, other; one awk pass, newest N only | user | read-only | same (SYNC.md:58) | implemented | bin/history.sh:125-175; SYNC.md:58, 99 |
| LNX-HIST-02 | (log) | n/a | `$XDG_STATE_HOME/mole/operations.log`, row `[ts] [cmd run=<id>] ACTION path (detail)`; rotates at 5 MiB to `.1`; control bytes escaped; actions seen: REMOVED, TRASHED, SKIPPED, FAILED, DISABLED, ENABLED, REBUILT, TASK_FAILED | user | R6; skipped when `MO_NO_OPLOG=1` or dry run | same format as Mac/Windows | implemented | lib/core/log.sh:11-68 |
| LNX-UPD-01 | update | (none) | Query `install.sh --latest`; verify by checksum; install via a temp copy of `install.sh`; no downgrade, even with `--force`; nightly->stable switch | user | installed-tree only | same | implemented | bin/update.sh:35-114; lib/core/base.sh:183-205 |
| LNX-UPD-02 | update | `--force` | Reinstall same version | user | | same | implemented | bin/update.sh:100-111 |
| LNX-UPD-03 | update | `--nightly` | Install latest `linux` branch commit from source; builds Go helpers; downloads checksum-checked Go into Mole cache if no Go | user | | same | implemented | bin/update.sh:57-74; install.sh:187-261 |
| LNX-UPD-04 | menu | (none) | Update notice in main menu, cached daily, lookup in background | user | | same | implemented | lib/core/update_notice.sh; mole:186-187 |
| LNX-REM-01 | remove | `--dry-run`, `--yes` | Removes `~/.local/share/mole`, `~/.local/bin/{mo,mole}` links that point at it, Mole's config, cache and state dirs; refuses non-installed trees | user | exact paths, `rm -rf --one-file-system` | same | implemented | bin/remove.sh:31-75 |
| LNX-INST-01 | install.sh | see section 1 | Release or nightly into `~/.local/share/mole`, links `mo` and `mole` into `~/.local/bin`; x86_64 and aarch64 only; sha256 verified; no sudo | user | | differs (Mac uses brew or script) | implemented | install.sh:6-11, 41-46, 165, 336-380 |

---

## 3. Linux cleanup target catalog

Every rule is `section|label|root|owners|selector`; children of root are removed, root never (lib/clean/rules.sh:1-20). Roots pass `mole_rule_root_ok` (rules.sh:37-66): not `$HOME`, no `.`/`..`, not protected, not an ancestor of an XDG base, `.local`, `.var/app`, `.ssh`, `.gnupg`. XDG bases: `XDG_CACHE_HOME`, `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `XDG_STATE_HOME` with defaults (lib/core/base.sh:60-76). Trailing slash dropped.

### 3.1 User caches under `$XDG_CACHE_HOME` (default `~/.cache`)

| Category | Representative paths | Owner process | Source |
|---|---|---|---|
| Thumbnails | `~/.cache/thumbnails` | none | rules.sh:81 |
| Browsers | `chromium`, `google-chrome`, `BraveSoftware`, `microsoft-edge`, `vivaldi`, `mozilla/firefox`, `zen` | `chromium`, `chrome`, `brave`, `msedge`, `vivaldi-bin`, `firefox`, `zen,zen-bin` | rules.sh:72-77 |
| WebKitGTK | `~/.cache/webkitgtk/{WebKitCache,MediaCache}` (CacheStorage kept) | `WebKitNetworkProcess,WebKitWebProcess` | rules.sh:120-125 |
| Apps | `thunderbird/*/cache2`, `~/.local/share/TelegramDesktop/tdata/user_data*/{cache,media_cache}`, `gnome-software` | thunderbird/betterbird, telegram-desktop, gnome-software | rules.sh:128-152 |
| AUR builds | `yay`, `paru` | yay/paru/makepkg/pacman | rules.sh:89-90 |
| Unknown | any other `~/.cache/*` over 100 MB: top 5 reported only | n/a | clean.sh:394-422 |

### 3.2 Browser and Electron data under `$XDG_CONFIG_HOME`

Chromium-family profile roots (11 specs): `chromium`, `google-chrome` (+`-beta`, `-unstable`), `BraveSoftware/Brave-Browser` (+Beta, Nightly), `microsoft-edge` (+`-beta`, `-dev`), `vivaldi` (rules.sh:207-218). Browser-level leaves (10): `ShaderCache GrShaderCache GraphiteDawnCache GPUPersistentCache component_crx_cache OptGuideOnDeviceModel OptGuideOnDeviceClassifierModel optimization_guide_model_store Crashpad/completed "Crash Reports/completed"`. Profile leaves (7): `GPUCache "Code Cache" DawnCache DawnGraphiteCache DawnWebGPUCache GrShaderCache GraphiteDawnCache` (rules.sh:198-206).

Electron apps (11): Code, Code - OSS, Code - Insiders, VSCodium, Cursor, obsidian, discord, Slack, Signal, Typora, heroic (rules.sh:250-261); leaves `Cache "Code Cache" GPUCache DawnCache DawnGraphiteCache DawnWebGPUCache Crashpad/completed` (248), editors add `CachedData CachedExtensionVSIXs logs` (249). Editor obsolete extensions at `~/.vscode/extensions`, `~/.vscode-insiders/extensions`, `~/.vscode-oss/extensions`, `~/.cursor/extensions` (277-285).

### 3.3 Sandboxed apps

| Format | Path | Owner | Notes | Source |
|---|---|---|---|---|
| Flatpak | `~/.var/app/<id>/cache` (never `config/`, `data/`) | `flatpak:<id>` via `flatpak ps` | name narrowing table in delete.sh:146-182 | rules.sh:403-413 |
| Snap | `~/snap/<name>/common/.cache`, `~/snap/<name>/<rev>/.cache` (current link and links skipped) | `snap:<name>` via cgroup `snap.<name>.*` | | rules.sh:419-433 |

### 3.4 Package-manager and system targets (the only root actions)

| Target | Command | Root need | Guard | Source |
|---|---|---|---|---|
| pacman cache `/var/cache/pacman/pkg` | `sudo paccache -rk2` (preview `paccache -dk2`) | sudo, needs `paccache` (pacman-contrib) | skip if `/var/lib/pacman/db.lck`; keeps 2 versions | clean.sh:440, 647-682 |
| apt cache `/var/cache/apt/archives` | `sudo apt-get autoclean` | sudo | skip if apt, apt-get, aptitude, dpkg, synaptic or unattended-upgrade runs or unknown; never `clean` | clean.sh:445, 712-758; base.sh:356-385 |
| journald `/var/log/journal` | `sudo journalctl --vacuum-time=2weeks` | sudo | retention floor; never `--vacuum-size=1` | clean.sh:686-705 |
| Flatpak user `~/.local/share/flatpak` | `flatpak uninstall --user --unused --noninteractive` | none | flatpak pins | clean.sh:522-544 |
| Flatpak system `/var/lib/flatpak` | report command only | would need sudo | not run | clean.sh:550-556 |
| Snap disabled revisions `/var/lib/snapd/snaps` | report command only | would need sudo | rollback set kept | clean.sh:447, 764-784 |
| Core dumps `/var/lib/systemd/coredump` | report only | root | never removed | clean.sh:559-568 |
| Docker/Podman | report `system prune` hint | n/a | never pruned | clean.sh:598-617 |
| Trash | `~/.local/share/Trash/{files,info}` emptied directly | none | R3 | clean.sh:350-390 |
| `/tmp`, `/var/tmp` | no rule (tmpfs on Arch; AGENTS.md:28 describes the rule that would apply) | n/a | | AGENTS.md:28 |
| dnf/yum/zypper/pip/nix caches | not present | n/a | | grep: no `dnf`, `zypper`, `nix-collect` in code |

### 3.5 Developer caches (rules.sh:288-388)

npm `~/.npm/_cacache`; Yarn, pnpm metadata, Corepack (`~/.cache/node/corepack`, `COREPACK_HOME` honored only if it holds `v1/` or `lastKnownGood.json`); Bun `~/.bun/install/cache`; Deno `deps remote gen npm registries` under `~/.cache/deno` (only); node-gyp (`~/.cache/node-gyp`, `~/.node-gyp`); Electron downloads; ESLint, Prettier, Vite, webpack, Turbo `~/.turbo/cache`, Parcel `~/.parcel-cache`, TypeScript, Prisma; sccache; pip, uv (via `uv cache prune`), Poetry `cache`+`artifacts`, pyenv `~/.pyenv/cache`, pre-commit, Ruff, mypy; Go build `~/.cache/go-build`; Cargo `registry/cache` (`CARGO_HOME` honored inside home), rustup `downloads`; Zig; Composer, Bundler `~/.bundle/cache`, RubyGems specs and `~/.gem/ruby/*/cache/*.gem`; CPAN `~/.cpan/build`; Hex; opam download cache; Gradle `notifications` only (Mac whitelists `caches`/`daemon`, SYNC.md:32); Terraform, kubectl `~/.kube/cache`, Docker Buildx `~/.docker/buildx/cache`, AWS CLI cache, gcloud logs, Azure logs; GitLab Runner, GitHub Actions `act`, CircleCI, SonarQube; OpenCode; Oh My Zsh cache; Android `build-cache` and `cache`; mise download cache; Bazel `~/.cache/bazel/_bazel_<user>/cache/repos`; JetBrains and Android Studio `log/` only (`~/.cache/JetBrains/*/log`, `~/.cache/Google/AndroidStudio*/log`; LocalHistory untouched, rules.sh:390-400); Claude Code and Cursor Agent old versions.

Never touched (AGENTS.md:29): `/var/log` live logs, package DBs, `~/.cache/doc`, `~/.ollama`, `~/.cache/huggingface`, `~/.cache/lm-studio`, `~/.cache/ms-playwright`, `~/.cache/Cypress`, `~/.cache/puppeteer`, mount points, `/boot /usr /bin /sbin /lib* /etc /var/lib /proc /sys /dev /run /nix/store`. The protected-path function adds `/root`, `/snap`, XDG bases, `~/.local`, `~/.ssh`, `~/.gnupg`, keyrings, systemd user dir, autostart, `.var/app` (delete.sh:20-37).

### 3.6 Purge target table (lib/purge/scan.sh:33-75)

`node_modules`, `target`, `build`, `dist`, `.venv`, `venv`, `.tox`, `.nox`, `.pytest_cache`, `.mypy_cache`, `.ruff_cache` (last three need `CACHEDIR.TAG` signature, scan.sh:100-107), `.gradle`, `.cxx`, `.next`, `.nuxt`, `.output`, `.turbo`, `.parcel-cache`, `.angular`, `.svelte-kit`, `.astro`, `.expo`, `coverage`, `vendor`, `bin`, `obj`, `.dart_tool`, `.zig-cache`, `zig-out`, `.build`, `.terragrunt-cache`, `_build`, `.stack-work`, `cmake-build-*`. Each requires its tool's project file beside it (package.json, Cargo.toml, pom.xml, build.gradle, pubspec.yaml, CMakeLists.txt, pyproject.toml, composer.json, `*.csproj`, Package.swift, mix.exs, ...).

---

## 4. Uninstall on Linux

### 4.1 Inventory (what counts as an app)

An app is something with a launcher (inventory.sh:4-10). `mole_list_apps` merges, sorted by name (inventory.sh:522-532):

| Kind | Source | How listed | Size | Remover |
|---|---|---|---|---|
| `pacman` / `aur` | `pacman -Qqe` explicit packages owning a `/usr/share/applications/*.desktop` (`pacman -Qo`); AUR if in `pacman -Qqm` | skips `NoDisplay`/`Hidden` | `pacman -Qi` Installed Size | `sudo pacman -Rns` (bin/uninstall.sh:620) |
| `apt` | `apt-mark showmanual` ∩ `dpkg-query -S` owner of a launcher | | `dpkg-query` Installed-Size | `sudo apt-get remove` (uninstall.sh:637), never purge |
| `snap` | `snap list` ∩ launchers in `/var/lib/snapd/desktop/applications/<snap>_<app>.desktop` | bases, cores, themes have none | `.snap` file bytes | `sudo snap remove` (uninstall.sh:673) |
| `flatpak` | `flatpak list --app --columns=application,name,installation`, scopes `user`, `system`, custom | | `du` of installation dir | `flatpak uninstall --user/--system/--installation=X` (uninstall.sh:690) |
| `webapp`, `tui` | Omarchy launchers in `~/.local/share/applications` (Exec contains `omarchy-launch-webapp`, `omarchy-webapp-handler`, or terminal `-e`) | | fixed 4 KB | Trash launcher+icon (uninstall.sh:707-726) |
| `appimage` | `~/.local/share/applications/*.desktop` whose Exec resolves to a user-owned `*.AppImage` in `$HOME` | | file size | Trash launcher, file, icon |
| `orphan` | `~/.var/app/<id>` with no installed app | `flatpak list` must succeed | `du` | Trash folder |

Citations: inventory.sh:160-223 (pacman), 234-293 (apt), 301-331 (snap), 412-430 (flatpak), 435-464 (web/tui), 468-485 (AppImage), 506-520 (orphan). Packages that ship no `.desktop` file are not listed. `.rpm`/`dnf`/`zypper`/Nix/Homebrew-on-Linux/`.run`/`/opt` apps: not supported (grep finds no code; AGENTS.md:17 "Fedora's dnf comes later"; docs/ledger.md:67 open item).

Debian-specific: the launcher named like the package supplies the display name, then `debian-<pkg>`, then the first (inventory.sh:273-277). Package-owned-by-two or diversion lines skipped (inventory.sh:230-233).

### 4.2 Protection

`mole_package_protected` (inventory.sh:141-158): `base`, `base-devel`, `linux*`, `systemd*`, `pacman*`, `sudo`, shells (bash, zsh, fish, nushell, dash, ...), the login-shell package (resolved via `getent passwd` + `pacman -Qoq`/`dpkg-query -S`, inventory.sh:163-165, 237-244), Omarchy/Hyprland/uwsm/waybar/walker/mako stack, xdg-desktop-portal, pipewire/wireplumber, networkmanager/iwd, bluez, polkit, gnome-keyring, display managers, mesa/vulkan/nvidia, yay/paru, flatpak, gnome-shell/session, plasma, kwin, xorg, wayland; Debian/Ubuntu: apt, dpkg, snapd, gdm3, `ubuntu-*`, policykit-1, xwayland, update managers, unattended-upgrades, packagekit, firmware-updater. Missing from the list (gaps for porting): KDE/others beyond plasma/kwin, `grub`, `networkmanager` variants on Fedora, `dnf`, `rpm`, `selinux-*`.

Leftover protection: `mole_is_protected_path` (delete.sh:20-37); shared names never leftovers: applications, autostart, dconf, fontconfig, fonts, gtk-*, icons, keyrings, mime, pipewire, pulse, ssh, gnupg, steam, systemd, themes, thumbnails, cargo, rustup, npm, java, git, docker, kube, aws, omarchy, hypr, ... (inventory.sh:695-706). A path any other installed app also maps to is kept with debug "also used by" (uninstall.sh:299-311, 337-340).

### 4.3 Running check

pacman/aur/apt: `Exec=` program name via `mole_process_running` (pgrep -x, 15-byte comm rule, base.sh:231-248) then any process whose exe or argv names a file of the package (`pacman -Qlq` / `dpkg-query -L`, base.sh:282-326, inventory.sh:337-351); flatpak: `flatpak ps`; snap: `/proc` cgroup scan (base.sh:329-355); AppImage: exe path (base.sh:413). Unknown (rc 2) blocks (uninstall.sh:470-471).

### 4.4 Leftover matching (exact identifiers only)

`mole_app_leftovers` (inventory.sh:726-790). IDs = package id + `.desktop` basenames + one `StartupWMClass` only if exactly one launcher on the system declares it and it is not electron/java/wine/python/steam_app/chromium/qemu (inventory.sh:92-126). For each ID (regex `^[A-Za-z0-9][A-Za-z0-9._+-]*$`, not a shared name):

- cache (checked): `$XDG_CACHE_HOME/<id>`, `$XDG_CONFIG_HOME/autostart/<id>.desktop`, `$XDG_DATA_HOME/applications/<id>.desktop` (non-AppImage).
- data (unchecked unless `--with-data`): `$XDG_CONFIG_HOME/<id>`, `$XDG_DATA_HOME/<id>`, `$XDG_STATE_HOME/<id>`, `~/.<id>`, `~/.config/systemd/user/<id>.{service,timer,socket}`.
- Flatpak: `~/.var/app/<id>`, `$XDG_DATA_HOME/flatpak/overrides/<id>` (inventory.sh:751-755).
- AUR: yay/paru build folder by pkgbase if it holds a PKGBUILD (inventory.sh:779-788, 710-721).
- alias table: Arch `code` -> `~/.vscode-oss`, `visual-studio-code-bin` -> `~/.vscode`, vscodium, cursor, firefox -> `~/.mozilla` (claimed by every Mozilla-family package so offered only when none stay), thunderbird, zen (inventory.sh:595-615).
- xdg-ninja table: 74 hand-reviewed home paths (MIT, THIRD-PARTY-NOTICES.md:6-13; inventory.sh:623-690; SYNC.md:72), credential stores removed after review.
- Snap, webapp, tui, orphan: no leftover scan (snap deletes `~/snap/<name>` itself, inventory.sh:750).

Held back from preselection even with `--with-data` (inventory.sh:796-870): browser profiles (Chromium, Gecko, Brave, Vivaldi, Edge, Opera, Waterfox, Floorp, LibreWolf, Tor, Mullvad, native and Flatpak), mail (`~/.thunderbird`), Spotify cache, model stores (`~/.ollama`, huggingface, lm-studio), game libraries (Flatpak Steam, Bottles), VMs (GNOME Boxes), local-only chat (Viber, Signal, Element, Session, Jami).

Everything goes to Trash via `mole_trash uninstall` except package removal (uninstall.sh:938).

---

## 5. Optimize/maintenance and status metrics

### 5.1 Optimize tasks

See LNX-OPT-04..21. Mechanisms: read-only tool calls with `LC_ALL=C` (`mole_c`, base.sh:80), `timeout` wrapper for slow probes, SQLite opened `mode=ro&immutable=1` so no `-wal`/`-shm` is created (tasks.sh:508-535). System config touched: none. Hints name root commands to run: `sudo pacman -Rns`, `sudo rm /var/lib/pacman/db.lck`, `sudo pacdiff`, `sudo systemctl enable --now fstrim.timer`, `sudo systemctl disable --now <pm>`.

### 5.2 Status metric collection on Linux (cmd/status)

| Card | Source | File:line |
|---|---|---|
| CPU usage, load, logical CPUs | gopsutil (reads `/proc/stat`, `/proc/loadavg`) | metrics_cpu.go:12 |
| CPU clock (max of `scaling_cur_freq`) | `/sys/devices/system/cpu/cpu[0-9]*/cpufreq/scaling_cur_freq` | metrics_cpu.go:181-183 |
| Memory, swap | gopsutil; memory pressure PSI `/proc/pressure/memory`; zram totals from `/sys/block/zram*/mm_stat` | metrics_memory.go:10, 24, 67, 129-132 |
| Disks, mounts | `/proc/self/mountinfo` (pseudo-fs filter list), `/proc/diskstats` physical name filter excluding loop, ram, dm-, md, zram, sr, fd, nbd | metrics_disk.go:23, 42, 161, 217-222 |
| Network | gopsutil + `/proc/net/dev`, default route `/proc/net/route` | metrics_network.go:67, 150 |
| Battery, UPS, peripherals | `/sys/class/power_supply` (`energy_*` else `charge_*`, `power_now`, `energy_full_design`) | metrics_battery.go:30, 105-108, 205-209 |
| Temperatures, fans | `/sys/class/hwmon/hwmon*` then `/sys/class/thermal/thermal_zone*` | metrics_thermal.go:53-70, 100-101, 211 |
| GPU | `/sys/class/drm` cards; `gpu_busy_percent` (amdgpu); `nvidia-smi --query-gpu=...` per card PCI address; sleeping dGPU not woken | metrics_gpu.go:63, 132-166, 217-231 |
| Per-process GPU memory | `/proc/<pid>/fdinfo/<fd>` DRM `drm-client-id` and memory keys, only for shown rows | metrics_drm.go:14-37, 141 |
| btrfs allocation | `/sys/fs/btrfs/<fsid>/allocation`, `devices`; red when metadata 90% and under 1 GiB unallocated | metrics_btrfs.go:58; AGENTS.md:40 |
| Hardware identity | `/etc/os-release` or `/usr/lib/os-release`, `/proc/cpuinfo`, `/sys/class/dmi/id` (treats `Default string`, `To be filled by O.E.M.`, `QEMU` as unknown) | metrics_hardware.go:21, 116, 148-151 |
| Processes and apps | `/proc/<pid>/stat`, `/proc/<pid>/cgroup` grouped by systemd `app-*.scope` / `app-flatpak-<id>`, desktop files from `/usr/share`, `/var/lib/flatpak/exports/share/applications` for names | metrics_process.go:54-65; metrics_apps.go:36-122, 151-158 |
| Uptime | `/proc/uptime` | metrics_process.go:105 |
| Health score | Weighted score with PSI memory warn/critical 5/15; disk critical only at 90% used and under 30 GB free | metrics_health.go:15; SYNC.md:85 |
| Restart hint | container detection via `/run/.containerenv` | metrics_restart.go:19 |
| Power draw (RAPL/supply watts), peripherals on Power card | | SYNC.md:39 |

No root is used; unreadable sources degrade quietly.

---

## 6. Analyze and status TUIs: shared or separate code

Same Go code lineage as macOS but forked: "the Go TUIs under `cmd/` were forked from `main` at the branch point and evolve independently" (AGENTS.md:3, SYNC.md:29). Not shared at runtime or at build time. Stack: Bubble Tea (`tea.NewProgram(..., tea.WithAltScreen())`, cmd/analyze/main.go:118, cmd/status/main.go:303), lipgloss (cmd/status/main.go:14), `charmbracelet/x/ansi` for grapheme width (SYNC.md:84, 97), `golang.org/x/sys/unix` in xdgtrash (xdgtrash.go:28), gopsutil in status. `cmd/analyze/main.go` and `cmd/status/main.go` are 285 and 409 lines; both reimplement help text by hand (usageText) so Go's flag package never prints the binary name (cmd/analyze/main.go:23-26).

Linux-specific deltas from the Mac fork: mountinfo volume table, GNU `du --exclude`, btrfs subvolume logic, Trash through `xdgtrash`, `xdg-open` for open/reveal/preview (reveal opens the parent directory since xdg-open cannot select, cmd/analyze/main.go:255-285), Linux insights list (insights.go:60-117), cgroup app grouping and PSI in status. The shell UI (main menu, paginated multi-select, spinner) is Bash (lib/ui/menu_paginated.sh, 1118 lines; lib/core/ui.sh) copied from the Mac branch "as is" (SYNC.md:27).

Bash orchestrates; the Go binaries are leaf programs. `trash-go` is the one Go helper used by bash (lib/core/delete.sh:96-123).

---

## 7. Deletion mechanics, distro detection, privilege

### 7.1 Three sinks (lib/core/delete.sh:1-10)

| Sink | Used by | Mechanism | Source |
|---|---|---|---|
| `mole_trash` | uninstall leftovers and own files, analyze delete | `bin/trash-go -- <paths>`; only an `ok\t<src>\t<dest>` line counts as moved; a helper that cannot run is failure; each path rechecked with R3 first; dry run returns before the helper | delete.sh:77-123; cmd/trash/main.go:23-50 |
| `mole_remove_cache` | clean | `rm -rf --one-file-system` on a child of an allowlisted root, only if `mole_cache_child_ok` (path under root, plain home item, same device as root, app-cache narrowing) | delete.sh:219-251 |
| `mole_remove_artifact` | purge | `rm -rf --one-file-system` after identity recheck | delete.sh:258-303 |
| `rm -rf --one-file-system` on exact Mole paths | remove | | bin/remove.sh:72-74 |
| distro tools | clean, uninstall | `paccache`, `apt-get autoclean`, `journalctl --vacuum-time`, `pacman -Rns`, `apt-get remove`, `snap remove`, `flatpak uninstall`, `uv cache prune` | see 3.4 |

Caches are removed directly, not Trashed, because moving a cache to Trash frees nothing (delete.sh:5-6, AGENTS.md:26). Full-tree copying across devices never happens.

### 7.2 freedesktop Trash (internal/xdgtrash/xdgtrash.go)

- Spec 1.0 plus fail-closed rules (header :3-17). Home Trash is `$XDG_DATA_HOME/Trash` (:70-76), used when the item shares a device with it (:150-153). Otherwise the volume's `$topdir/.Trash-$uid`, topdir found by walking up while device stays the same (:155-181); no topdir or `/` is an error. Note: the spec's `$topdir/.Trash/$uid` variant is not used, only `.Trash-$uid`.
- `.trashinfo` created with `O_EXCL` first to reserve the name (`name`, `name.2`, ...up to 1000), fsynced; `Path=` percent-encoded, absolute for home Trash, relative to topdir for volume Trash; `DeletionDate=` local time `2006-01-02T15:04:05` (:118-135, 234-264, 286-302). No `directorysizes` cache file is written.
- Move uses `renameat2(RENAME_NOREPLACE)`, falls back to a checked rename on EINVAL/ENOSYS (:268-283). `EXDEV` becomes "on another filesystem than its Trash; left in place" and the reserved info is removed (:138-144).
- Refuses `/`, `$HOME`, the Trash itself, a folder holding the Trash (:88-108), Trash dirs that are symlinks, not owned, or group/other writable (:207-229). Symlinks are moved as links, never followed (:78).
- `gio trash` is documented as a fallback (AGENTS.md:26) but no code calls it (grep: no `gio trash`, no `trash-put`).
- Restoring: left to the user's file manager through the `.trashinfo`; Mole has no restore command.

### 7.3 Distro detection

None as such. There is no `ID`/`ID_LIKE` switch. Behavior keys off tool presence: `command -v pacman`, `apt-get`+`dpkg-query`, `flatpak`, `snap`, `journalctl`, `paccache`, `docker`, `podman`, `coredumpctl` (bin/clean.sh:441-451, 454-456; bin/optimize.sh:180-191; inventory.sh:161, 235, 302, 413). `/etc/os-release` is only parsed for the version banner (mole:39-51) and status's hardware card (metrics_hardware.go:148-151). Omarchy specifics (Hyprland autostart, web apps, `omarchy-*` protected packages) are coded in. Immutable distros (Fedora Atomic `/var/home`) are handled for path realpath only (delete.sh:273-277, scan.sh:604, cmd/analyze/mounts.go:178). Architecture: x86_64 and aarch64 only (install.sh:43-46).

### 7.4 Privilege handling

- Mole refuses to run as root (mole:6-9; common.sh:14-19). No `pkexec`, `doas` or polkit code exists anywhere (grep). Only `sudo`.
- `mole_sudo_ready` (common.sh:29-63): `sudo -n true`, else needs a TTY and writable `/dev/tty`, then `sudo -v -p "<arrow> Password: "`; one failure sets `MOLE_SUDO_FAILED` and no re-prompt in the run; keepalive subshell refreshes `sudo -n -v` every 50 s; `mole_sudo_end` runs `sudo -k` only when Mole obtained the timestamp (common.sh:67-76).
- Sudo is used for: `paccache -rk2`, `apt-get autoclean`, `journalctl --vacuum-time=2weeks`, `pacman -Rns`, `apt-get remove`, `snap remove`, system/custom `flatpak uninstall` (clean.sh:673, 694, 751; uninstall.sh:620, 637, 673, 700). No sudo in optimize, analyze, status, purge, history.
- Environment cannot redirect system targets: `MOLE_SYS_APPS_DIR`, `MOLE_SNAP_DESKTOP_DIR`, `MOLE_SNAP_SNAPS_DIR`, `MOLE_PACMAN_DBPATH`, `MOLE_DOCKER_SYSTEM_SOCKETS`, `APT_ARCHIVES_DIR`, `SNAPD_SNAPS_DIR` are plain globals (AGENTS.md:31, clean.sh:443-451, inventory.sh:225-229, 299-300).

---

## 8. Missing on Linux versus macOS

From SYNC.md and code. macOS side is `main`; not present in this tree, so Mac commands are from SYNC.md references plus background knowledge (unverified).

| Gap | Evidence |
|---|---|
| `mo completion` (shell completion) | AGENTS.md:18 "stays out"; ledger.md:12; SYNC.md:40; no file |
| `mo installer` (cleanup of downloaded installers `.dmg/.pkg`) | absent from mole:91-103; (Mac feature, unverified) |
| `mo touchid` and Touch ID sudo | absent; SYNC.md:31 "Touch ID has no Linux counterpart" |
| Privileged Trash staging under `/Library` | SYNC.md:16 not applicable |
| Finder, TCC, LaunchServices, APFS clones | SYNC.md:17 not applicable |
| Shared bash core `lib/core/file_ops.sh` | SYNC.md:14 intentionally re-implemented |
| dnf/yum/zypper/rpm/Nix/emerge/apk support | AGENTS.md:17; ledger.md:11, 62, 67; grep shows no code |
| Spotify offline cache, Zoom, podman tmp, ms-playwright, Gradle `caches`/`daemon`, Copilot CLI, curl/wget | SYNC.md:32 not ported |
| `paccache -ruk0` (uninstalled packages) | SYNC.md:32 waits for maintainer |
| Purge: cloud-storage roots, dataless folders, `__pycache__` | SYNC.md:57 |
| Battery cycle-count health (Apple's rated 1000) | SYNC.md:85 not ported |
| Analyze delete-progress | SYNC.md:75 not applicable (single rename) |
| Analyze hidden-row skip loop | SYNC.md:107 not applicable |
| Per-item size-probe timeouts as in main #1998/#2004 | SYNC.md:108 Linux re-implements differently |
| Restore from Trash | no command; file manager only (section 7.2) |
| Timeshift snapshot detection | snapshots.sh:9-10, AGENTS.md:25: only snapper-on-btrfs detected |
| Homebrew, Xcode, simulators, iOS backups, Mail/Photos caches | macOS-only categories (unverified; none in rules.sh) |
| Windows-style features: none on Linux beyond those already ported | SYNC.md:53-60 |
| Real hardware gaps: per-process GPU on real GPU, AMD sysfs names, every battery driver | docs/ledger.md:49 |
| Known gaps: purge cannot read cwd of non-dumpable processes; `uninstall --yes` off-TTY cannot remove Flatpak; installs <= 0.6.0 cannot see new releases until reinstall (#1984) | ledger.md:68, 70, 73; SYNC.md:109 |

Linux-only extras (no Mac equivalent): snapper/btrfs honesty (SYNC.md:70), apt/Snap/Flatpak clean and uninstall, xdg-ninja leftovers (72), pacman reports (73), cgroup app grouping and btrfs allocation in status (74), journald/coredump/container reporting.

---

## 9. Porting notes (for a Rust TUI + CLI, macOS then Windows then Linux)

1. **Model the cleanup as a rules table**, not per-app functions. The Linux `section|label|root|owners|selector` row (rules.sh:1-20) is the right unit: a Rust `Rule { section, label, root, owners: Vec<Owner>, selector: Selector }` with `Owner::{Process, Cmd(regex), Flatpak, Snap}` and `Selector::{All, Versions(link), Obsolete, Suffix, Tool(uv)}` makes macOS and Windows tables share the executor. Keep the generators (profiles, editors, flatpak per-app) as functions that yield rules.
2. **One deletion funnel with three sinks and a typed path check.** Encode R2 protected, R3 plain-home-item, R7 same-device as a `SafePath` newtype constructed only by a checker; the executor accepts `SafePath` only. This removes the bash weakness of string checks repeated per caller.
3. **Dry run equals real run.** Size and count only children that `cache_child_ok` accepts (clean.sh:219-233); implement by making the executor take a `Mode::{Plan, Apply}` and return the same `Vec<Action>`.
4. **Owner probe tri-state** `Running | NotRunning | Unknown`, Unknown treated as running (clean.sh:94-137). Encode as an enum so unknown cannot be coerced to false. Replicate the 15-byte comm trap on Linux (base.sh:231-280); on macOS/Windows use native process APIs.
5. **Per-OS ports.** Trash: macOS needs `NSFileManager trashItem`/Finder semantics; Windows `IFileOperation` with recycle flag; Linux freedesktop (`.Trash-$uid` only, add `directorysizes` and `$topdir/.Trash/$uid` if you want full spec). Crate `trash` covers three but Mole's fail-closed rules (O_EXCL reservation, no cross-device copy, ownership/mode checks of the Trash dir) must be reimplemented or verified. Port xdgtrash.go (302 lines) directly.
6. **Root actions through distro commands only.** Keep a closed enum `PrivilegedAction` (`PaccacheKeep2`, `AptAutoclean`, `JournalVacuum(2w)`, `PacmanRemove(pkg)`, ...), each with a preview command, a busy check and a log line. On Linux, add dnf (`dnf clean packages`? not `clean all`), zypper, and rpm-ostree equivalents plus `pkexec` as a GUI-less-sudo fallback; Mole offers neither.
7. **Do not switch on distro ID.** Mole feature-detects by command presence; keep that, and add an `os-release` `ID`/`ID_LIKE` read for display and for choosing which package-manager inventory to try first. Immutable distros (Silverblue, Atomic) need `/var/home` realpath handling (delete.sh:273-277).
8. **Inventory design.** Sources are `.desktop` files ∩ package manager ownership; protect session-critical packages by a closed list plus the login-shell package. For dnf use `rpm -qf` on `/usr/share/applications/*.desktop` and `dnf repoquery --userinstalled`. AppImage discovery relies on launchers in `~/.local/share/applications` (AppImageLauncher, Gear Lever); AppImages with no launcher are invisible.
9. **Leftovers by exact ID** only (R-exact): derive ID set from package id, desktop ids, one unique `StartupWMClass`; emit the fixed path templates (inventory.sh:757-770); keep a `shared_names` deny list and a `holdback` list as data files, not code. Reuse the 74 xdg-ninja paths under MIT with the notice (THIRD-PARTY-NOTICES.md).
10. **Status TUI**: the Go collector is a clean reference. Copy sysfs/procfs sources from section 5.2 but replace gopsutil with `sysinfo`/`procfs` crates; keep the non-waking dGPU rule, PSI-based health, disk-critical rule (90% and under 30 GB), cgroup app grouping. Bubble Tea maps to `ratatui`; analyze's volume rules (mountinfo, btrfs by source+fstype not `st_dev`, `.snapshots` excluded, hardlink `(dev,ino)` dedupe) are the load-bearing part.
11. **Log format.** Reuse `[ts] [cmd run=<id>] ACTION path (detail)` with control-byte escaping and 5 MiB rotation (log.sh:23-68) so history/JSON stay compatible across all three Mole CLIs.
12. **Test shape to copy**: bash/bats tests each in own HOME with fake `flatpak`, `pgrep`, Trash helper (AGENTS.md:45); tests files: clean, delete, history, install, optimize, purge, snapshots, uninstall, update (.bats), PTY tests for the sudo gate (`tests/clean_sudo_gate_pty.py`) and menu search (`tests/menu_search_pty.py`); Go tests alongside every `cmd/*` file. Real tools are exercised only in CI (`real-tools.yml`, ~2 GB flathub pull). In Rust use fake-bin dirs on PATH plus a pty crate (`expectrl`).
13. **Pitfalls Mole documented** (worth carrying over): parse with `LC_ALL=C`, never localized text (AGENTS.md:33); sudo gate must drop typed keys so a password is never read as a skip (SYNC.md:93); a `~/.cache` symlink or `XDG_CACHE_HOME` pointing inside config/data would turn "Chromium cache" into the profile (clean.sh:147-165); `uv`/`pnpm`/`go` hardlink caches mean `du` overstates freed space (clean.sh:298); snap `remove` deletes `~/snap/<name>`; Flatpak prompts "n" off-TTY.
14. **Priority**: Linux slice milestone = rules table + SafePath sink + xdgtrash port + status/analyze collectors; defer dnf/zypper and AppImage-without-launcher; reports-only tasks (optimize) are cheap and high value; add `pkexec` only if a non-terminal front end is wanted.
15. **License**: this branch is GPL-3.0 (README.md:173) with a trademark file (TRADEMARK.md). An original Rust implementation may reuse the *ideas* and safety contract but must not copy code or the names; the xdg-ninja data is MIT and needs its notice.
