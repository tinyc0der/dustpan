# Mole macOS (main @ 5dd93fe3): optimize, core CLI, UI plumbing inventory

Source root: `../mole` (VERSION 1.72.0, `mole:115`). All `file:line` refs are relative to it. Mole macOS is Bash 3.2 plus two Go TUIs (`cmd/analyze`, `cmd/status`, built to `analyze-go` / `status-go`, launched by `bin/analyze.sh:9-15` and `bin/status.sh:9-15`). Linux and Windows live on other branches and were not read.

## 1. Complete command and flag reference (all commands)

### 1.1 Dispatcher (`mole`, `mo`)

`mo` is a 7-line launcher that `exec`s `mole "$@"` (`mo:6-7`). `mole` refuses to run as root (`mole:10-13`; same guard repeats in `bin/optimize.sh:10`, `bin/touchid.sh:10`, `bin/completion.sh:4`). It refuses an inaccessible cwd except for `--version/-V/version/--help/-h/help` (`mole:21-31`).

| Token(s) | Routes to | Mechanism | Source |
|---|---|---|---|
| (none) | `check_for_updates` then interactive main menu | in-process | `mole:420-423` |
| `optimize`, `optimise` | `bin/optimize.sh` | `run_mole_command` | `mole:326-328` |
| `clean` | `bin/clean.sh` | `run_mole_command` | `mole:329-331` |
| `uninstall` | `bin/uninstall.sh` | `run_mole_command` | `mole:332-334` |
| `analyze`, `analyse` | `bin/analyze.sh` -> `analyze-go` | `run_mole_command` | `mole:335-337` |
| `status` | `bin/status.sh` -> `status-go` | `run_mole_command` | `mole:338-340` |
| `purge` | `bin/purge.sh` | `run_mole_command` | `mole:341-343` |
| `installer` | `bin/installer.sh` | `run_mole_command` | `mole:344-346` |
| `slim` | `bin/slim.sh` | `run_mole_command` | `mole:347-349` |
| `touchid` | `bin/touchid.sh` | `exec` | `mole:350-352` |
| `completion` | `bin/completion.sh` | `exec` | `mole:353-355` |
| `history` | `bin/history.sh` | `exec`, dispatched BEFORE `common.sh` is sourced | `mole:94-107` |
| `update` | `update_mole` (in-process) | parses own flags | `mole:356-385` |
| `remove` | `remove_mole` (in-process) | parses own flags | `mole:386-411` |
| `help`, `--help`, `-h` | `show_help` | in-process | `mole:412-415` |
| `version`, `--version`, `-V` | `show_version` | in-process | `mole:416-419` |
| anything else | stderr "Unknown command: X" + "Use 'mole --help'..." exit 1 | | `mole:424-430` |

Canonical command list (15 entries, shared by help and completions): `clean, uninstall, optimize, analyze, status, history, purge, installer, slim, touchid, completion, update, remove, help, version` with one-line descriptions at `lib/core/commands.sh:4-20`.

Global flag: `--debug` is stripped from argv anywhere on the line by `mole_collect_cli_args` and exported as `MO_DEBUG=1` (`mole:77-92,320`; history path `mole:100`). Debug effects: stderr `[DEBUG]` lines, `~/Library/Logs/mole/mole_debug_session.log` (`lib/core/log.sh:25,207-214`), system-info header (`log.sh:443-481`), risk-level lines (`log.sh:422-440`). There is no global `--dry-run`; it is per command (`-n` short form).

`run_mole_command` (`lib/manage/update.sh:1030-1120`): when stdin/stdout/stderr are not all TTYs, or argv contains `--json|--ndjson|--watch|--help|-h|--version|-V|--list`, it plain-`exec`s (1032-1038). Otherwise it runs the check-for-updates background lookup, launches the child through a Perl signal-forwarding launcher, forwards INT/TERM/HUP, and prints the cached update notice to stderr after the child exits with rc<128 (1104-1118).

### 1.2 Per-command flags (every flag, from the source of each parser)

| Command | Flag / arg | Meaning | Source |
|---|---|---|---|
| `clean` | `-h`, `--help` | help | `bin/clean.sh:2214`; text `lib/core/help.sh:3-14` |
| | `--debug` | debug | `clean.sh:2218` |
| | `-n`, `--dry-run` | preview | `clean.sh:2221` |
| | `--external PATH` | clean `.TemporaryItems`/`.DS_Store`/`._*` on an external volume | `clean.sh:2225-2232` |
| | `--whitelist` | open protected-paths editor and exit | `clean.sh:2233-2237` |
| | `--select`, `--categories`, `--exclude` | removed; print guidance, exit 1 | `clean.sh:2238-2242` |
| | other `-*` / positional | error exit 1 | `clean.sh:2243-2252` |
| `uninstall` | `-h`, `--help` | help | `bin/uninstall.sh:2107`; `help.sh:56-82` |
| | `--debug` | | `uninstall.sh:2111` |
| | `-n`, `--dry-run` | | `uninstall.sh:2114` |
| | `--permanent` | `MOLE_DELETE_MODE=permanent` (skip Trash) | `uninstall.sh:2117` |
| | `--with-data` | preselect personal data rows | `uninstall.sh:2120` |
| | `--list` | read-only list of apps and accepted names | `uninstall.sh:2123,2146` |
| | `--whitelist` | rejected | `uninstall.sh:2126-2131` |
| | `APP_NAME ...` positional | uninstall by name | `uninstall.sh:2137` |
| `optimize` | `-h`, `--help` | | `bin/optimize.sh:215`; `help.sh:27-37` |
| | `--debug` | | `optimize.sh:219` |
| | `-n`, `--dry-run` | `MOLE_DRY_RUN=1` | `optimize.sh:222` |
| | `--whitelist` | task opt-out editor, exit | `optimize.sh:225-228` |
| `analyze` (Go) | `-h/--help`, `--json`, `[PATH]`, env `MO_ANALYZE_PATH` | | `cmd/analyze/main.go:22,55,71`; completion lists `--json --help -h` (`completion.sh:21`) |
| `status` (Go) | `--json`, `--proc-cpu-threshold` (float, default 100), `--proc-cpu-window` (duration, default 5m), `--proc-cpu-alerts` (bool, default true), `--watch`, `--interval` (e.g. `2s`, default 1s), `-h/--help` | | `cmd/status/main.go:26-33,101-104,359`; NOT in any completion list |
| `history` | `--json` | JSON export | `bin/history.sh:28` |
| | `--limit N` (1-200, default 20) | | `history.sh:31-41`; `lib/core/history.sh:17-18` |
| | `-h`, `--help` | | `history.sh:42` |
| `purge` | `--paths` | edit scan dirs | `bin/purge.sh:335` |
| | `-n`, `--dry-run` | | `purge.sh:347` |
| | `--yes` | unattended | `purge.sh:350` |
| | `--include-empty` | show zero-size | `purge.sh:353` |
| | `--debug`, `-h/--help` | | `purge.sh:344,340` |
| `installer` | `-n/--dry-run`, `--debug`, `-h/--help` | | `bin/installer.sh:1005,1002,998`; `help.sh:16-25` |
| `slim` | `-n/--dry-run`, `--debug`, `-h/--help` | | `bin/slim.sh:853,852,848` |
| `touchid` | subcommands `enable`, `disable`, `status` (one only) | | `bin/touchid.sh:364-371` |
| | `-n`, `--dry-run` | | `touchid.sh:357` |
| | `-h`, `--help` | | `touchid.sh:360`; `help.sh:39-54` |
| `completion` | `bash`, `zsh`, `fish` | emit script | `bin/completion.sh:339-456` |
| | (none) | auto-install into shell rc | `completion.sh:112-336` |
| | `-n`, `--dry-run` | preview rc edits | `completion.sh:92-109` |
| | `-h`, `--help`, other | usage (exit 0 for help, 1 otherwise) | `completion.sh:457-487` |
| `update` | `-f`, `--force` | reinstall stable | `mole:361` |
| | `--nightly` | install main-branch build | `mole:362` |
| | `-h`, `--help` | | `mole:363-375` |
| `remove` | `-n`, `--dry-run` | | `mole:390` |
| | `-h`, `--help` | | `mole:391-402` |
| `install.sh` | `--prefix DIR`, `--config DIR`, `--update`, `-v/--verbose` | | `install.sh:1161,1169,1177,1181` |

Counts: 15 canonical subcommands (`commands.sh:4-20`) plus 2 spelling aliases (`optimise`, `analyse`) and 6 alias tokens (`help/--help/-h`, `version/--version/-V`, each counted once as the same command). Distinct flags: global 1 (`--debug`), dispatcher-level 4 (`--help/-h`, `--version/-V`), command flags 38 distinct (long forms: `--dry-run`, `--debug`, `--whitelist`, `--external`, `--select`*, `--categories`*, `--exclude`*, `--permanent`, `--with-data`, `--list`, `--json`, `--limit`, `--paths`, `--yes`, `--include-empty`, `--force`, `--nightly`, `--json`(analyze/status), `--watch`, `--interval`, `--proc-cpu-threshold`, `--proc-cpu-window`, `--proc-cpu-alerts`, plus install.sh `--prefix --config --update --verbose`; `*` = removed, error only). Subcommand words under touchid (3) and completion (3).

### 1.3 Completion coverage gaps (port should do better)

Completion option lists are hard-coded at `completion.sh:20-24`: only `clean`, `analyze`, `history`, `purge`, `completion` have flag completion. Missing: `optimize`, `uninstall` (`--list`, `--permanent`, `--with-data`), `installer`, `slim`, `touchid` subcommands, `update`, `remove`, `status` flags, `analyze`'s `--json` only. Bash completion uses `complete -F _mole_completions mole mo` (`completion.sh:386`); zsh `#compdef mole mo` with `_describe` (`390-440`); fish emits per-command `complete -f -c` lines and a `__fish_mole_no_subcommand` function for both `mole` and `mo` (`31-63,442-456`).

### 1.4 Environment variables that act as user-visible knobs

`MO_DEBUG`, `MOLE_DRY_RUN`, `MO_NO_OPLOG=1` (disable operations log, `log.sh:32,113,257`), `NO_COLOR` (`base.sh:23-40`), `MOLE_DELETE_MODE` (`permanent` default | `trash`, `file_ops.sh:2386,2407`), `MOLE_DELETE_LOG` (`file_ops.sh:2391,3436`), `MOLE_CONFIG_DIR` (`remove.sh:133`, `update.sh:402,813`), `MOLE_ENABLE_DISK_VERIFY=1` (`tasks.sh:1530`), `MOLE_OPTIMIZE_SPOTLIGHT_SLOW_SEC` (default 3, `tasks.sh:906`), `MOLE_ASSUME_VPN_ACTIVE` (`tasks.sh:98`), `MOLE_PERIODIC_LOG` (`tasks.sh:1323`), `MOLE_TIMEOUT_{QUICK_DETECT 2,SHORT_QUERY 3,MEDIUM_PROBE 5,PKG_LIST 10,PKG_CLEANUP 20,DISK_VERIFY 30,HINT_SCAN 15}_SEC` (`timeouts.sh:61-67`), `MOLE_LOOKUP_CONNECT_TIMEOUT/MAX_TIME` (update lookups), `MOLE_PURGE_YES`, `MOLE_PURGE_INCLUDE_EMPTY` (set by purge flags). Test-only: `MOLE_TEST_MODE`, `MOLE_TEST_NO_AUTH` (hard-deny sudo, `sudo.sh:124,372`).

## 2. Feature table (my area)

Privilege column: `user` = no elevation; `sudo` = `sudo` invoked internally; `FDA` = needs Full Disk Access to see data.

| ID | Command | Flags/options | Behavior | Privilege | Safety rule | Source |
|---|---|---|---|---|---|---|
| CORE-01 | `mole`/`mo` router | none | Resolve own dir through symlinks/Homebrew layout, route subcommands | user; refuses root | root refused; cwd must resolve | `mole:10-66,325-432` |
| CORE-02 | `--debug` | global | Strip anywhere in argv, set `MO_DEBUG=1`, write debug session log | user | n/a | `mole:77-92,320` |
| CORE-03 | `help`/`--help`/`-h` | | Banner + command table + example lines + `OPTIONS --debug` | user | n/a | `mole:412`; `update.sh:1198-1233` |
| CORE-04 | `version`/`--version`/`-V` | | Prints version, channel (+commit if nightly), macOS ver, arch, kernel, SIP, disk free, install method (Manual/Homebrew/Nix), shell; quiet on SIGPIPE | user | n/a | `update.sh:1134-1196` |
| CORE-05 | interactive main menu | none | 5-entry arrow menu, see section 4 | user | n/a | `mole:157-314` |
| CORE-06 | update notice | automatic | Daily background lookup (1h retry on unknown), cached in `~/.cache/mole/update_message`, shown in menu and after commands | user | throttle key = cksum(version,channel,commit,SCRIPT_DIR); stale notices dropped; Nix installs skipped | `update.sh:913-1026` |
| UPD-01 | `mo update` | none | Resolve latest stable (GitHub redirect, API, git tags fallback), download install.sh at tag `V<ver>`, run staged installer, verify installed generation; Homebrew installs run `brew update` + `brew upgrade mole` | sudo only if install dir not writable (keepalive + handoff probe) | fail-closed version check; self-heal reinstall from `main` streamed to bash; single-flight lock; Nix install refused | `update.sh:1236-1603`; `common.sh:188-` |
| UPD-02 | `mo update --force`/`-f` | | Reinstall stable even if current | same | same | `mole:361`; `update.sh:1360-1370,1563` |
| UPD-03 | `mo update --nightly` | | Install `main` build pinned to resolved 40-hex commit; "Already on latest nightly" if same 7-char; refused on Homebrew | same | requires resolved commit unless `--force` | `update.sh:1262-1326,1549-1562` |
| UPD-04 | stable after nightly | | Switches channel back (`install_channel` receipt) | same | | `update.sh:1352-1359` |
| UPD-05 | post-update highlights | automatic | Prints up to 4 changelog titles of the installed release | user | silent off-TTY and on nightly | `update.sh` (`mole_print_release_highlights`, called 1507/1512) |
| RM-01 | `mo remove` | `-n/--dry-run` | Detect Homebrew/manual launchers (`mole`,`mo` in PATH, `/usr/local/bin`, `~/.local/bin`, `/opt/local/bin`), shell completion lines, fish completions, `~/.cache/mole`, config dir; confirm Enter/ESC; `brew uninstall --force mole`, `rm` launchers (sudo if dir not writable), config moved to Trash `~/.Trash/mole-config[-N]`; logs kept | sudo only for unwritable launcher dir | launcher verified by byte signature (no exec); needs a real TTY; custom config roots kept for manual review; `~/Library/Logs/mole` kept; Nix refused | `remove.sh:17-474` |
| HIST-01 | `mo history` | none | Last 20 sessions + 20 deletion-audit rows, newest first | user | read-only | `bin/history.sh:60-67`; `history.sh:782-863` |
| HIST-02 | `mo history --json` | | `{logs{operations,deletions}, limit, sessions[], deletions[]}`; session fields `command, run_id, attribution(command|run|ambiguous), started_at, ended_at, items, size, operation_count, failed_tasks, actions{removed,trashed,skipped,failed,rebuilt,other}`; deletion fields `timestamp, mode, status, size_kb|null, path`; invalid UTF-8 -> U+FFFD | user | read-only | `history.sh:865-954` |
| HIST-03 | `mo history --limit N` | 1-200 | Caps both lists | user | invalid -> exit 1 | `history.sh:31-41,70-114` |
| TOUCHID-01 | `mo touchid` | none | Show status then Enter to enable/disable, Q/ESC quit | sudo | | `touchid.sh:301-348` |
| TOUCHID-02 | `mo touchid status` | | Prints enabled / not configured | user | read-only | `touchid.sh:82-88` |
| TOUCHID-03 | `mo touchid enable` | `--dry-run` | Preferred: write `auth sufficient pam_tid.so` into `/etc/pam.d/sudo_local` (create 444 root:wheel, or append via `install -m 444 -o root -g wheel`), migrate/remove legacy line from `/etc/pam.d/sudo`; legacy fallback inserts after first comment block with one-time `.mole-backup` copy | sudo | warns if hardware unsupported (bioutil / arm64 / Intel >=2018) and prompts y/N; verifies content changed; logs `REBUILT` | `touchid.sh:34-37,54-75,91-225` |
| TOUCHID-04 | `mo touchid disable` | `--dry-run` | Remove `pam_tid.so` lines from sudo_local and/or sudo | sudo | backup before touching `/etc/pam.d/sudo`; logs `REBUILT` | `touchid.sh:228-298` |
| TOUCHID-05 | PAM detection | | `mole_pam_enables_touchid`: line with no `#` before `pam_tid.so` counts | user | commented line is not enablement | `sudo.sh:44-49` |
| TOUCHID-06 | Touch ID hint | automatic | After a password-authenticated sudo, if fingerprints enrolled (`bioutil -c`) and PAM disabled, prints "Tip: run mo touchid..." | user | Mole never edits PAM on its own | `sudo.sh:90-98` |
| COMP-01 | `mo completion` | `--dry-run` | Detect shell from `$SHELL`; fish writes `~/.config/fish/completions/{mole,mo}.fish`; bash/zsh append a marked eval line to `~/.bashrc`/`~/.bash_profile`/`~/.zshrc`; Enter/Q confirm on first install; normalizes existing line | user | rc symlink chain followed, mode preserved, only exact marker lines touched | `completion.sh:112-336`; `base.sh:588` |
| COMP-02 | `mo completion bash|zsh|fish` | | Emit script to stdout | user | | `completion.sh:339-456` |
| OPT-00 | `mo optimize` | `--dry-run/-n`, `--whitelist`, `--debug`, `-h` | Orchestrator: clear screen, header, health JSON (needs `bc`), active-whitelist note, diagnostics, sudo gate, run 19 tasks, summary | sudo (once, up front) | see 3 | `bin/optimize.sh:208-334` |
| OPT-WL | `mo optimize --whitelist` | | Multi-select editor of task IDs to skip | user | retired IDs dropped on load/save | `optimize.sh:225`; `whitelist.sh:7-12,189-198,339-524` |
| OPT-DIAG | performance diagnosis | automatic | Two `ps` CPU samples 1s apart; five process families; memory/swap; idle VM; runaway process; mounted-image detach offer | user (`hdiutil detach` offered) | read-only except detach, which needs Enter | see 3.3 |
| OPT-01..19 | optimize tasks | | See section 3.1 | | | `catalog.sh:31-87` |
| WL-01 | `mo clean --whitelist` (shared editor, `manage_whitelist clean`) | | ~70 cache rows + custom paths; preselect current; save to `~/.config/mole/whitelist` | user | `SAFETY_WHITELIST_PATTERNS` merged always on load; absence must be confirmed ENOENT, unreadable file aborts | `whitelist.sh:91-187,209-319,339-524`; `base.sh:171-230` |
| WL-02 | `mo purge --paths` editor | | `~/.config/mole/purge_paths` | user | | `lib/manage/purge_paths.sh:16-121` |
| UI-01 | selection menu (paginated) | | Used by uninstall, installer, slim, purge | user | see section 6 | `lib/ui/menu_paginated.sh` |
| UI-02 | selection menu (simple) | | Used by whitelist editors (`whitelist.sh:17`) | user | | `lib/ui/menu_simple.sh` |

## 3. `mo optimize` in detail

### 3.1 Task catalog (19 tasks, run in this order; `catalog.sh:31-87`)

Every task ends in exactly one outcome: `applied | unchanged | skipped | unavailable | attention | failed` (`outcomes.sh:11-24`). A task that errors on any eligible sub-operation is `failed`; counts resolve via `optimize_task_result_from_counts` (`outcomes.sh:82-104`). All catalog entries must be `safe=true` (`catalog.sh:144-147`). Whitelisting is by action ID via `is_whitelisted` (`tasks.sh:2146-2153`; `network_optimization` also honors the legacy ID `system_maintenance`, `tasks.sh:2145`). Exact-match only, no glob (`whitelist.sh:321-337`). A whitelisted task prints `Skipped (whitelisted): <Health Name>` and reports `skipped`.

| ID | Action ID (health name) | What it does | sudo | Dry-run output | Source |
|---|---|---|---|---|---|
| OPT-01 | `cache_refresh` ("Finder Cache Refresh") | `qlmanage -r cache`, `qlmanage -r`; delete `~/Library/Caches/com.apple.QuickLook.thumbnailcache`, `com.apple.iconservices.store`, `com.apple.iconservices` via `safe_remove` (skips protected paths) | no | reports both refreshes "would" via yellow ○; no qlmanage call, no delete | `tasks.sh:209-309` |
| OPT-02 | `saved_state_cleanup` ("App State Cleanup") | `find ~/Library/Saved\ Application\ State -type d -name '*.savedState' -mtime +30` then remove (protected paths skipped) | no | `opt_msg` only; no deletion, because `safe_remove` returns early under `MOLE_DRY_RUN=1` (`file_ops.sh:1558`) | `tasks.sh:316-374`; `base.sh:161` |
| OPT-03 | `fix_broken_configs` ("Broken Config Repair") | `plutil -lint` (absolute `/usr/bin/plutil`) all `*.plist` in `~/Library/Preferences` (maxdepth 1) and `ByHost` (recursive) in batches of 512; remove corrupt ones; 15 s scan budget (partial -> failed) | no | removal suppressed by `safe_remove` dry-run | `tasks.sh:382-422`; `maintenance.sh:7-129` |
| OPT-04 | `network_optimization` ("Network Cache Refresh") | `sudo dscacheutil -flushcache`, then `sudo launchctl kill SIGHUP system/<label>` where label read from the SIP plist (`com.apple.mDNSResponder` or `.reloaded`); macOS 27 refusal falls back once to `sudo -u _mdnsresponder killall -u _mdnsresponder -HUP mDNSResponder` | sudo | yes (counts as available) | `tasks.sh:164-206,425-463` |
| OPT-05 | `sqlite_vacuum` ("Database Optimization") | VACUUM Mail `V*/MailData/Envelope Index*`, `Messages/chat.db`, `Safari/History.db`, `TopSites.db` | no (Messages needs FDA) | counts what would be vacuumed; skips integrity check | `tasks.sh:529-713` |
| OPT-06 | `prevent_network_dsstore` ("Prevent Finder .DS_Store") | `defaults write com.apple.desktopservices DSDontWriteNetworkStores -bool true` and `DSDontWriteUSBStores` | no | no write | `tasks.sh:1100-1140` |
| OPT-07 | `legacy_overrides_audit` ("Legacy Overrides") | delete explicit keys `NSAppSleepDisabled` (-g) and `com.apple.frameworks.diskimages skip-verify{,-locked,-remote}` when truthy | no | "Would remove override: ..." | `tasks.sh:1149-1214` |
| OPT-08 | `network_stack_optimize` ("Network Stack Refresh") | health check `route -n get default` + `dscacheutil -q host -a name example.com`; only if one fails: `sudo route -n flush`, `sudo arp -a -d` | sudo | yes | `tasks.sh:721-823` |
| OPT-09 | `disk_permissions_repair` ("Permission Repair") | only if `$HOME` owner != user or `$HOME`, `~/Library`, `~/Library/Preferences` not writable: `sudo diskutil resetUserPermissions / $(id -u)` | sudo | yes | `tasks.sh:69-89,826-877` |
| OPT-10 | `spotlight_index_optimize` ("Spotlight Optimization") | `mdutil -s /`; on AC power run two `mdfind kMDItemFSName == 'Applications'` probes; if both slower than 3 s: `sudo mdutil -E /` (1-2 h rebuild) | sudo (rebuild only) | "rebuild started" message, no sudo | `tasks.sh:880-978` |
| OPT-11 | `spotlight_orphan_rules_cleanup` ("Spotlight Orphan Rules") | Parse `EnabledPreferenceRules` snapshot of `~/Library/Preferences/com.apple.Spotlight.plist` (exact-case domain), drop reverse-DNS lowercase bundle IDs whose app is gone (never `System.*`, `com.apple.*`, `Custom.*`), recheck snapshot, `defaults write`/`delete` | no | "Would remove N orphan Spotlight rule(s)" | `tasks.sh:988-1094` |
| OPT-12 | `periodic_maintenance` ("Periodic Maintenance") | if `periodic` exists and `/var/log/daily.out` older than 7 d: `sudo periodic daily weekly monthly` | sudo | yes | `tasks.sh:1315-1363` |
| OPT-13 | `shared_file_list_repair` ("Shared File Lists") | `plutil -lint` `*.sfl2/3/4` under `~/Library/Application Support/com.apple.sharedfilelist`, remove corrupt, never `ApplicationRecentDocuments`; permission-only scan errors -> `unavailable` | no | counts without removing | `tasks.sh:1366-1448` |
| OPT-14 | `disk_verify` ("Disk Health") | `diskutil verifyVolume /` (30 s cap). Disabled unless `MOLE_ENABLE_DISK_VERIFY=1` (can freeze the kernel on APFS errors); always skipped in dry-run | no | skipped | `tasks.sh:1529-1568` |
| OPT-15 | `login_items_audit` ("Login Items Audit" in whitelist menu) | AppleScript via System Events lists login items, resolves each by path, Spotlight name variants, filesystem scan of `/Applications` + `~/Applications` (depth 6), and, only with an existing sudo session, `sudo -n sfltool dumpbtm`; reports only | none (uses existing session) | same (read-only) | `tasks.sh:1641-2125` |
| OPT-16 | `quarantine_cleanup` ("Quarantine Database Cleanup") | `sqlite3 ~/Library/Preferences/com.apple.LaunchServices.QuarantineEventsV2 "DELETE FROM LSQuarantineEvent; VACUUM;"` | no | counts rows, no write | `tasks.sh:466-526` |
| OPT-17 | `launch_agents_cleanup` ("Launch Agents Cleanup") | Report `~/Library/LaunchAgents/*.plist` whose absolute `Program`/`ProgramArguments[0]` is missing (ignores unmounted `/Volumes`); never deletes/unloads | no | same | `tasks.sh:1236-1310` |
| OPT-18 | `notification_cleanup` ("Notifications") | If Notification Center DB > 50 MB: `DELETE FROM record WHERE delivered_date < now-30d; VACUUM;` then `launchctl kill SIGTERM gui/$UID/com.apple.notificationcenterui.agent` | no | message only | `tasks.sh:1456-1523` |
| OPT-19 | `coreduet_cleanup` ("Usage Data") | If `knowledgeC.db`+WAL+SHM >= 100 MB: `DELETE FROM ZOBJECT WHERE ZCREATIONDATE < now-90d; VACUUM;` (never unlink WAL/SHM) | no | message only | `tasks.sh:1571-1639` |

Sudo tasks: OPT-04, 08, 09, 10 (rebuild branch), 12; opportunistic: OPT-15. One upfront `ensure_sudo_session "System optimization requires admin access"` with keepalive (`optimize.sh:306-313`); if denied, `MOLE_OPTIMIZE_SUDO_AVAILABLE=false` and each sudo task reports `skipped` (admin access required). In dry-run it is forced `true` so every task expands (`optimize.sh:307-308`).

Retired/removed (not present in any registry; do not port): `dock_refresh`, `memory_pressure_relief`, `launch_services_rebuild` (names kept only as retired whitelist IDs, `whitelist.sh:9`); `opt_maintenance_scripts`, `opt_radio_refresh`, `opt_swap_cleanup`, `opt_startup_cache`, `opt_local_snapshots`, `opt_startup_items_cleanup`, `opt_dyld_cache_update`, `opt_system_services_refresh` (removed, with reasons in comments at `tasks.sh:311,313,376-380,715-718`). There is no memory purge and no LaunchServices rebuild on main (`.claude/rules/optimize.md`: a rebuild made a running Network Extension VPN look reinstalled). Third-party app upgrades are out of scope for optimize (`AGENTS.md:57`).

VPN guard: DNS flush and route/ARP flush skip if a VPN is active or unknown, via `scutil --nc list` Connected plus default-route `utun*` (`tasks.sh:97-155`).

### 3.2 Run flow, output and summary

1. Header `Optimize` in bold purple (`optimize.sh:32-35`); dry-run banner `○ DRY RUN MODE, No files will be modified` (yellow, `249`).
2. Spinner "Collecting system info..." while `generate_health_json` builds `{memory_used_gb, memory_total_gb, disk_used_gb, disk_total_gb, disk_used_percent, uptime_days, optimizations[{category,name,description,action,safe}]}` (`lib/check/health_json.sh:111-173`); `lib/check` has only this file and is reachable only from `mo optimize` (`optimize.sh:29,262`). Line `⚙ System  X/Y GB RAM | A/B GB Disk | Uptime Nd` (`optimize.sh:153`).
3. Active whitelist line when 1-3 patterns (`optimize.sh:286-295`).
4. Diagnostics (3.3), then per task: blue `➤ <health name>` header, blank line between tasks (`optimize.sh:157-166`), 2-space-indented result lines `✓ msg` (green) or `○ msg` (yellow, dry-run) via `opt_msg`, `◎ msg` yellow for warnings (`tasks.sh:20-27`).
5. Summary box via `print_summary_block` (`optimize.sh:55-126`). Real: title `Optimization Complete`; `Applied N optimizations[, <key stat>]` (key stat precedence: `X cache cleaned` > `N databases optimized` > `N configs repaired`); outcome line `A unchanged | B skipped | C unavailable | D need attention | E failed`; final line `Review the warnings above` or `Optimization pass complete`. Dry run: title `Dry Run Complete, No Changes Made`, `Would apply N optimizations`, `Run without --dry-run to apply these changes`.
6. Exit non-zero iff any task `failed` (`optimize.sh:325-333`). Operations log gets `TASK_FAILED` and `REBUILT` rows per task (not in dry-run for REBUILT) plus session start/end (`optimize.sh:168-200`).

### 3.3 Diagnostics (`lib/optimize/diagnostics.sh`)

Header `Performance diagnosis`. Two `ps -Aceo pcpu=,command=` samples 1 s apart; families `cloudshell|syspolicyd|windowserver|spotlight (mds, mdworker, mds_stores, mdbulkimport)|coresim_disk_images`, sustained if both samples >= 25% (`6,342-377`). Extra read-only checks: swap >= 50% used or free memory < 15% prints top holders > 256 MB (`456-589`); VM with > 2 GB RSS (Docker via `docker ps -q` on local endpoint only) (`594-633`); runaway process >= 25% lifetime CPU over >= 12 h, excluding `kernel_task`, `WindowServer`, `mds*`, `syspolicyd`, `mdworker*` (`638-676`). Only on sustained `syspolicyd` pressure: show `spctl --status`, list mounted `.dmg/.iso/.img/.cdr/.sparse*` under `/Volumes`, offer `hdiutil detach` with `Detach now? Enter confirm / Space cancel` (`414-433,293-340`); system-managed CoreSimulator/cryptex mounts are never targets (`183-200`). Healthy output: `✓ No sustained high-CPU bottleneck detected` (`390`).

### 3.4 Whitelist (optimize)

File `~/.config/mole/whitelist_optimize` (legacy `whitelist_checks` read once then migrated, `whitelist.sh:20-23,209-234,302-309`). Header `# Mole Optimization Whitelist - These checks will be skipped during optimization`; one action ID per line; `#` comments; retired IDs dropped (`whitelist.sh:48-50,65-67`). Menu rows come from `MOLE_OPTIMIZE_WHITELIST_NAMES` (`whitelist.sh:190-198`); title `Whitelist Manager, Select optimize tasks to ignore` + gray `Edit: ~/.config/mole/whitelist_optimize` (`366-367`). Path patterns are also accepted in the file and consulted by preference repair, plist repair, mounted-image detach, and legacy override audit through `is_path_whitelisted` (`maintenance.sh:95`, `diagnostics.sh:220-224`, `tasks.sh:1194`). Result summary: `Whitelist Updated` / `Protected N caches` / `Config: <path>` (`whitelist.sh:511-523`).

## 4. Interactive main menu (`mole:157-314`)

- Entry: `mo` with no args after `check_for_updates` (`mole:420-423`).
- Rendering: cursor home `\033[H`, banner (green ASCII "Mole" logo + `https://mole.fit` in blue + tagline `Deep clean and optimize your Mac.` in green, `update.sh:1123-1132`, `mole:116`), optional update notice (blank line then message; trailing `mo update`/`mo update --nightly` colored green, `mole:147-154,177-182`), then five rows.
- Rows (`mole:186-190`): `1. Clean        Free up disk space`, `2. Uninstall    Remove apps completely`, `3. Optimize     Refresh caches and services`, `4. Analyze      Explore disk usage`, `5. Status       Monitor system health`. Selected row: cyan `➤ N. text`; others two-space indent (`ui.sh:326-336`).
- Controls line (gray, TTY only; `mole:131-143`): `↑↓  |  Enter  |  M More  |  V Version  [|  T TouchID]  [|  U Update]  |  Q Quit`. `T TouchID` shown only when Touch ID is NOT configured; `U Update` only when a cached update notice exists.
- Keys (decoded by `read_key`, `ui.sh:168-277`): `UP`/`DOWN` arrows or `k`/`j` clamp 1..5 (`mole:260-261`); `Enter` runs highlighted row (`262-270`); digits `1-5` run that row directly (`271-285`); `M` -> `clear`, `show_help`, exit 0 (`286-291`); `V` -> `clear`, `show_version`, exit 0 (`292-297`); `T` -> `exec bin/touchid.sh` (always handled, even if hidden, `298-301`); `U` -> `update_mole` only if notice present (`302-308`); `Q`, Ctrl-C (`\x03`), EOF, and a lone Esc (1 s ESC wait) -> restore stty, show cursor, exit 0 (`309`, `ui.sh:186-218`); INT trap does the same (`mole:238`). Other keys (`h/l/g/G/R/Delete/chars`) are no-ops.
- Loop: 1 s idle tick (`read_key 1` returns non-zero on timeout) re-reads the update cache and redraws only if the notice changed (`mole:241-257`). After a launched command returns, `drain_pending_input` (perl `tcflush`, `ui.sh:281-323`) and the menu redraws. Cursor is shown and 0.1 s of input drained before each command (`mole:232-236`).
- Dispatch: row commands go through `run_mole_command`, so the menu keeps running after the command ends; `M`, `V`, `U` exit the process after output.

## 5. Config, state, log files

| Path | Purpose | Writer / reader | Source |
|---|---|---|---|
| `~/.config/mole/whitelist` | clean protections (patterns, one per line, `~` allowed, `FINDER_METADATA` sentinel) | `save_whitelist_patterns`, `load_whitelist` | `whitelist.sh:21,31-88,209-319` |
| `~/.config/mole/whitelist_optimize` | optimize task opt-outs | same | `whitelist.sh:22` |
| `~/.config/mole/whitelist_checks` | legacy optimize file; read then migrated | | `whitelist.sh:23,226-234,303-309` |
| `~/.config/mole/purge_paths` | extra purge scan roots | `manage_purge_paths` | `purge_paths.sh:16` |
| `~/.config/mole/status_prefs` | status TUI prefs (Go) | `cmd/status/prefs.go:28` | |
| `~/.config/mole/install_channel` | install receipt (CHANNEL, COMMIT_HASH, INSTALL_RECEIPT) in `$MOLE_CONFIG_DIR` or `SCRIPT_DIR` | install.sh, `get_install_channel` | `update.sh:808-846` |
| `~/Library/Logs/mole/mole.log` (+`.old`, 1 MB rotate) | info/success/warn/error log | `log.sh:24,102-110` | |
| `~/Library/Logs/mole/operations.log` (+`.old`, 5 MB rotate) | audit of every Mac-changing action; `[ts] [cmd run=ID] ACTION path (detail)`; session markers `# ========== <cmd> session started/ended ... items, size ==========`; dry-run sessions named `<cmd>-dry-run`; actions `REMOVED, TRASHED, SKIPPED, FAILED, REBUILT, TASK_FAILED` | `log_operation`, read by `mo history` | `log.sh:26,288-359`; `history.sh:604-627` |
| `~/Library/Logs/mole/deletions.log` | per-item TSV `<iso_ts>\t<mode>\t<size_kb>\t<status>\t<path>`; `MOLE_DELETE_LOG` overrides | `file_ops.sh:2395,3436`; read at `history.sh:629-647` | |
| `~/Library/Logs/mole/mole_debug_session.log` | truncated each `--debug` session | `log.sh:25,443-481` | |
| `~/.cache/mole/update_message`, `version_check` | update notice and throttle (`key ts 3600|86400`) | `update.sh:959-1022` | |
| `~/.cache/mole/tmp` | private temp root (700) pruned of Mole leftovers | `base.sh:1533-1566` | |
| `~/.cache/mole/uninstall_app_metadata_v5`, `installed_apps_cache`, `pkg_receipt_apps_v*`, `brew_last_cleanup`, `permissions_granted` | other commands' caches | grep of `lib bin` | `uninstall.sh:43` |
| `/etc/pam.d/sudo`, `/etc/pam.d/sudo_local` (+`sudo.mole-backup`) | Touch ID | `touchid.sh:29-32,189` | |
| `~/.zshrc`, `~/.bashrc`, `~/.bash_profile`, `~/.config/fish/config.fish`, `~/.config/fish/completions/{mole,mo}.fish` | shell completion | `completion.sh`, `remove.sh:47-70` | |

`mo remove` deletes `~/.cache/mole` and moves `~/.config/mole` to Trash, keeps `~/Library/Logs/mole` (`remove.sh:429-460`).

Defaults: `DEFAULT_WHITELIST_PATTERNS` (16 entries including Playwright/Cypress caches, Gradle, Ollama models, JetBrains, Surge, Mobile Documents) `base.sh:172-189`; `DEFAULT_OPTIMIZE_WHITELIST_PATTERNS` is empty (`191-192`); `SAFETY_WHITELIST_PATTERNS` always merged for clean (`199-`). A saved whitelist file REPLACES optional defaults (`whitelist.sh:53`).

Sudo plumbing (`lib/core/sudo.sh`): `request_sudo_access` tries `sudo -n`; else TTY flow: clamshell (lid closed) -> password prompt `➤ Enter your credentials:`; Touch ID configured -> single `sudo -v` showing `➤ <msg> , Touch ID or password`; no TTY -> `osascript` hidden-answer dialog titled "Mole" with caution icon (AppleScript-escaped message) (`120-218`). Keepalive subshell runs `sudo -n -v` every 30 s (5 s retry on failure) until parent exits (`255-291`). `ensure_sudo_session` establishes keepalive (`365-395`). All of it hard-denied under `MOLE_TEST_MODE`/`MOLE_TEST_NO_AUTH` (`124,372`).

Timeouts: all external probes go through `run_with_timeout` with bucketed constants (`timeouts.sh:61-67`); native `timeout` preferred, perl fallback, 124 = deadline.

## 6. Shared UI conventions worth keeping

- Colors (`base.sh:54-63`): green `0;32` success; blue bold `1;34` headings/info; cyan `0;36` selection; yellow `0;33` warnings; purple `0;35` prompts; purple bold `1;35` titles; red `0;31`; gray `38;5;244` hints. `NO_COLOR` non-empty, `TERM=dumb`, or non-TTY stdout disables all (`base.sh:23-40`).
- Symbols (`base.sh:106-119`): `◎` confirm and warning, `⚙` admin/system, `✓` success, `☻` error, `○` empty checkbox and dry-run, `●` selected checkbox, `•` list, `↳` sublist, `➤` arrow/cursor/prompt, `⊙` review/note, `↑ ↓` nav.
- Spinner: braille frames `⠖ ⠲ ⠴ ⠦`, 0.08 s, blue, to stderr, message swapped in place via a message file, truncated to `cols-8` by display width; non-TTY prints `| <msg>` with no animation (`ui.sh:355-530`).
- Dry-run: banner `○ DRY RUN MODE, <what> will not be modified`, yellow `○` rows, closing `Dry Run Complete, No Changes Made` box (optimize) or `Dry run complete, no changes made` (`optimize.sh:249`, `completion.sh:153`, `touchid.sh:380`, `remove.sh:274,310`).
- Confirmation phrasing: `➤ Press Enter to confirm, ESC to cancel:` (`remove.sh:349`); `➤ Enable completion for <shell>? Enter confirm / Q cancel:` (`completion.sh:169,284`); `◎ Press Enter to enable|disable, Q to quit:` (`touchid.sh:305,327`); `Detach now? Enter confirm / Space cancel:` (`diagnostics.sh:327`). Rule: Enter is yes, anything else cancels; EOF or non-TTY is NEVER Enter (`remove.sh:338-366`, `.claude/rules/install-update.md`). Passwords/typed characters never count as "skip" (`AGENTS.md:116`).
- Summary box: 70-col max `=` rule, blue title, detail lines, rule (`log.sh:488-528`).
- Next-action hints: `Run without --dry-run to apply these changes`, `Use 'mo <cmd> --help' for supported options.`, `Unknown <cmd> option: X` to stderr exit 1 (`optimize.sh:229-233`).
- Selection menus: paginated menu (`menu_paginated.sh`) title in purple bold + `N/M selected`, `➤ ● item` cursor row in cyan, `○`/`●` checkboxes, alt-screen support (`tput smcup/rmcup`), page size = terminal height minus header/footer (max 50), keys: `↑↓`/`j k` move, `h l`/`←→` page, `g g`/`G` and Home/End top/bottom, `Space` select, `A` all (opt-in), `S` sort (date/name/size), `O` order, `/` search (multi-word AND, Esc or Q clears, `Ctrl+U` clear line, Backspace), `Enter` save, `Q` cancel (`menu_paginated.sh:552-624,703-1022`; `ui.sh:168-277`). Footer hints collapse in priority order when narrow. The simple menu (`menu_simple.sh:232`) footer: `↑↓ | Space | Enter Save | Q Cancel`; cases `ALL`/`NONE` exist (`289,294`) but `read_key` never emits them (dead).
- Output rhythm: title, loading line, content, one trailing blank (`.claude/rules/clean.md`); rows state what happened once; review rows use `⊙ label · value · fact`.
- Terminal safety: untrusted names (apps, plists, mounts) pass `mole_terminal_safe_text` (control bytes -> `?`) before display (`log.sh:216-222`); column math uses display width not bytes (`ui.sh:32-165`).
- Terminal hygiene: hide/show cursor (`ui.sh:28-29`), stty restore on INT/EXIT, perl `tcflush` drain after menu keys, ESC sequence timeouts 1 s (0.3 s `MOLE_KEY_SEQ_TIMEOUT` on newer bash, `ui.sh:21-23`).

## 7. Repo pieces: scope for a Rust port

| Piece | What it is | Verdict |
|---|---|---|
| `plugins/mole/` | Packaged agent-skill wrappers (Claude Code `.claude-plugin`, Codex `.codex-plugin`, DeepSeek Harness npm `dsh-mole` with `index.js`, locale JSON, icon) around the same SKILL.md (`plugins/mole/package.json`, `index.js`) | OUT of scope for the Rust port core; optional later distribution add-on if dustpan wants an agent skill |
| `skills/` | Plain Agent Skill (`skills/mole/SKILL.md` + `references/macos.md`, `windows.md`) teaching agents safe `mo` use (preview first, `--dry-run`, who runs what) | OUT of scope for code; reuse its consent/preview rules as docs and as a design checklist (`macos.md:29-39` classifies commands by who may run them) |
| `install.sh` | 71 KB manual installer: stages release/main source, verifies checksum + attestation, atomic install with lock, `--prefix/--config/--update/--verbose`, writes `install_channel` receipt (`install.sh:1161-1181`) | OUT of scope for Rust binary logic; replace with cargo-dist/Homebrew/winget/scoop packaging and a self-update channel |
| `flake.nix`, `package.nix`, `overlay.nix` (+`formatter.nix`, `flake.lock`) | Nix packaging: builds Go TUIs, reads VERSION out of `mole`; Nix installs refuse `mo update`/`mo remove` (`update.sh:1255`, `remove.sh:152`) | OUT of scope for the first milestones; optional later packaging target; carry over the "package-manager install refuses self-update" rule |
| `gemini-extension.json` | 5-line manifest naming the repo as a Gemini CLI extension (name mole, version 1.0.0) | OUT of scope (distribution metadata only) |
| `llms.txt` | Agent-facing install and usage guide listing brew/scoop and skill-install commands | OUT of scope; write dustpan's own equivalent when there is a release |
| `cmd/analyze`, `cmd/status` (Go) | The two TUIs (owned by other agents) | IN scope (other inventory slices) |
| `lib/check/health_json.sh` | Only used by `mo optimize` for the system-health line and JSON shape | IN scope, tiny (memory/disk/uptime facts) |

## 8. Porting notes

1. Model the optimize catalog as data: `struct Task { id, health_name, whitelist_name, description, needs_admin, os, run(ctx) -> Outcome }` with `enum Outcome { Applied, Unchanged, Skipped(reason), Unavailable(reason), Attention(msg), Failed(msg) }` (`outcomes.sh:11-24`). Summary, exit code, history and TUI rows all derive from the outcome list. Enforce the `safe=true` invariant and unique IDs at construction (`catalog.sh:113-159`).
2. One `Ctx { dry_run, whitelist, sudo: Option<SudoSession>, log }`. Dry-run must follow the same code path with the mutating sink disabled, and sudo availability is assumed true in dry-run so every task previews (`optimize.sh:307-308`).
3. Ask for admin once, up front, with a keepalive and a per-task "skipped: admin access required". On Windows the equivalent is a UAC-elevated helper process (do not re-elevate per task); on Linux polkit/`sudo -v`. Never run the whole app as root (`mole:10-13`).
4. Signalling services: never `killall` by name; macOS uses launchd label targeting (`optimize.md` rules for DNS and Notification Center). Windows analogue: service control by exact service name; Linux: `systemctl` unit names or `resolvectl flush-caches`.
5. macOS-only tasks (all of OPT-01..19 except the generic ones) need per-OS registries. Generic candidates across OSes: temp/cache refresh, DNS flush, broken-config lint, SQLite vacuum, saved-state cleanup, login-item audit, broken startup-entry audit, disk health verify (opt-in), permissions repair.
6. Use native APIs instead of shelling out where available: plist lint/read (the `plist` crate), SQLite `VACUUM`/`integrity_check` (`rusqlite`), `defaults`/cfprefsd must still be shelled for domains owned by running apps (cfprefsd keys domains by exact string, see `optimize.md`).
7. Safety invariants to carry over verbatim: lint with the system `plutil`-equivalent, unreadable file is unknown not corrupt (`maintenance.sh:82-90`); skip protected and whitelisted paths before attempting removal (`maintenance.sh:92-97`); never unlink SQLite WAL/SHM (`tasks.sh:1615-1620`); never delete live PowerLog DBs (`AGENTS.md:122`); refuse VPN-disruptive flushes when VPN state is active or unknown; time-box every probe and treat a timeout as skip/unavailable, a signal as cancel (`run_with_timeout` semantics, `timeouts.sh`).
8. Logging: keep the operations.log row format and the `<cmd>-dry-run` session naming so Mole Mac tooling and a future `dustpan history` stay compatible if desired; otherwise define the schema now with `run_id`, `command`, `action`, `path`, `detail`, `dry_run` fields and a JSONL sibling. Rotation at 1 MB / 5 MB, warn-once on unwritable audit log (`log.sh:54-59,97-124`).
9. `history --json` is a stable public contract (`history.sh:865-954`); replicate the field names. Session accounting should key on a run ID, not the command name (legacy ambiguity logic at `history.sh:116-306,395-540` exists only because old logs had no IDs; a greenfield port does not need it).
10. TUI: suggest `ratatui` + `crossterm`. Keep Mole's key model (arrows and `j/k`, digits as shortcuts, Enter, Q, Esc, `/` search, Space select, `S`/`O` sort), the 5-entry home menu, controls footer that hides hints by priority, and an update-available line. Drop Bash artifacts: perl `tcflush` drain, 1 s ESC timeout, `stty` restore (crossterm raw mode and event queue replace them). Respect `NO_COLOR` and non-TTY detection; keep gray hint text and the `✓ ○ ● ◎ ⊙ ➤ ↳` glyph set (provide ASCII fallback).
11. Confirmation: EOF or non-TTY never confirms (`remove.sh:338-366`); only a real key event. Machine commands (`--json`, `--list`, `--watch`) bypass launchers and stay silent on stdout (`update.sh:1038`).
12. Self-update / remove: keep fail-closed version checks, channel receipt (`stable`/`nightly`), package-manager detection (Homebrew, Nix, scoop, winget, cargo) that redirects to the owning tool, config to Trash, logs kept (`remove.sh:429-460`). Update lookups must be background, cached (1 day known / 1 h unknown), proxy-aware, and never block a command (`update.sh:975-1026`).
13. Touch ID is macOS-specific (PAM `pam_tid.so`). Windows analogue (Windows Hello for sudo) and Linux fprintd/PAM exist but are separate features; treat `touchid` as a macOS-only capability behind a trait. Preserve: prefer `sudo_local`, migrate legacy line, back up only `/etc/pam.d/sudo`, detect only uncommented lines, write via `install -m 444 -o root -g wheel` from a temp file (`touchid.sh:34-37,91-225`).
14. Completion: generate from the clap command tree (`clap_complete`) so all subcommands and flags are covered, fixing the gaps in 1.3. Keep the auto-install flow (marker line, symlink-aware rc edit, fish files), the `--dry-run` preview, and removal in `remove`.
15. Config paths: macOS keeps `~/.config/mole`, `~/Library/Logs/mole`, `~/.cache/mole`; dustpan should choose its own names and a single path module (`dirs`/`directories`), with `~` expansion and exact-string whitelist matching (no glob on task IDs, `whitelist.sh:321-337`). Whitelist saves must replace optional defaults but always merge hard-safety patterns (`base.sh:194-199`). Missing file must be a confirmed ENOENT, not a failed read (`whitelist.sh` via `mole_whitelist_file_state`, `base.sh:418`).
16. Hidden env tuning knobs (timeouts, `MOLE_ENABLE_DISK_VERIFY`, Spotlight threshold) are good candidates for a config file with the same defaults rather than env-only.
17. Diagnostics (section 3.3) are a distinct, read-only "performance diagnosis" feature worth keeping as a separate module shared by `optimize` and `status`; its thresholds (25% CPU sustained, 50% swap, 15% free, 2 GB idle VM, 25%/12 h runaway) are constants at `diagnostics.sh:6-7,446-450`.

## 9. Unresolved

- Linux and Windows branches not inspected; command parity there is unknown.
- `mole_print_release_highlights` body (called `update.sh:1507,1512`) and `update_via_homebrew` tail (`common.sh:188-`) read only partly.
- Go TUI behavior and key maps of `analyze`/`status` deliberately left to other slices; only their flag definitions are recorded.
- `bin/clean.sh` writes `~/.config/mole/clean-list.txt` (seen by grep, not read) and `.cache/mole/permissions_granted`; other agents own those.
- Whether `status`'s `--proc-cpu-*` flags are documented in `--help` text: usage text was seen only at its tail (`cmd/status/main.go:340-345`).
