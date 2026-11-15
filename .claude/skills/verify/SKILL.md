---
name: verify
description: Drive the real dustpan release binary in a pseudo-terminal to prove TUI and CLI behavior. Builds the binary, starts it at a given size, sends keys, mouse events and resizes, reads the screen back as text, times the first full frame, and takes Terminal.app window screenshots with agent-desktop. Use after any change to crates/dustpan, and before reporting a TUI lane as passed.
---

# Verify dustpan

dustpan is a Rust disk cleaner with a ratatui TUI and a clap CLI. The user touches a terminal. Bare `dustpan` in a terminal opens Home. With no TTY on stdin or stdout it prints `--help` and draws nothing. Every proof here runs `target/release/dustpan`, never a test double.

Run all commands from the repo root. Helper scripts live in `.claude/skills/verify/scripts/` and are executable.

## Launch

```bash
cargo build --release -p dustpan        # builds target/release/dustpan; nothing else to start
BIN=$PWD/target/release/dustpan
```

There is no server. Each drive starts its own isolated tmux pane (socket `dustpan-verify`, config ignored), so two drives never share state and your own tmux sessions are untouched. Ready means the last band of the screen has drawn:

```bash
V=.claude/skills/verify/scripts/pty-drive.sh
$V start home 120 40 -- $BIN        # new pane, 120 columns by 40 rows
$V wait home "q Quit" 5             # exit 0 when the footer is on screen
```

## Doctor

Read-only. Run it first when anything looks off.

```bash
$BIN --version                      # two lines: version and OS, arch, install method
git rev-parse --short HEAD; ls -l $BIN   # the binary is newer than the last source change
tmux -L dustpan-verify -f /dev/null list-sessions   # leftover panes from an earlier run
which mo                            # Mole, the reference for lane 1 and the perf baseline
```

If the screen is locked the GUI path fails (`ioreg -n Root -d1 -a | grep -A1 CGSSessionScreenIsLocked` says true, and `agent-desktop screenshot` times out with "Global application window inventory did not stabilize"). The text path still works.

## Drive

`pty-drive.sh <command> NAME [args]`. Keys are tmux key names.

| Goal | Command |
|---|---|
| Move down four, up one | `$V keys home Down Down Down Down k` |
| Open help, close it | `$V keys home '?'` then `$V keys home Escape` |
| Jump to entry 9 | `$V keys home 9` |
| Open the entry (toast) | `$V keys home Enter` |
| Settings toast | `$V keys home ,` |
| Quit / Quit now | `$V keys home q` / `$V keys home C-c` |
| Resize | `$V resize home 70 20` |
| Wheel at column 10, row 5 | `$V wheel home down 10 5` |
| Click column 10, row 5 | `$V click home 10 5` |
| Read the screen | `$V screen home` |
| Cursor, alternate screen, exit status | `$V info home` |
| Stop this pane only | `$V stop home` |

Wait after a key with `$V wait home "<text>" 3`, not a fixed sleep. For restore checks start a shell pane (`$V start sh 120 40`), type the binary path with `$V text sh "$BIN"` plus `$V keys sh Enter`, then quit it and read `$V info sh` and `stty -a` from the same shell.

The lane runner drives all of this for the M0-6 checklist lanes (1 to 8 and 10) and saves one capture per lane:

```bash
python3 .claude/skills/verify/scripts/home-lanes.py /tmp/swarm-m0-6/owner --bin $BIN
python3 .claude/skills/verify/scripts/home-lanes.py OUT --only 4      # one lane
```

Lane 9 (Windows Terminal) is run by the operator.

## Perf

First full frame means the footer text has arrived on the pty. `first-frame.py` forks the program in a pty and times fork to marker.

```bash
.claude/skills/verify/scripts/first-frame.py --marker 'Quit' --runs 10 -- $BIN
.claude/skills/verify/scripts/first-frame-compare.sh /tmp/swarm-m0-6/owner/perf.txt 10 50 $BIN
```

The compare script alternates `mo` (marker `Q Quit`) and dustpan, prints the `mo` median first, then dustpan's, and exits 1 over the budget (50 ms). `mo` is Mole's menu and makes network checks, so its figure is a reference, not a contract. Run perf with nothing else loading the machine.

## Window screenshots

Use Terminal.app and `agent-desktop` (load the `agent-desktop` skill first). Do not open Warp (it adds tabs to the user's own window) and avoid Ghostty. Close only windows this skill opened.

```bash
G=.claude/skills/verify/scripts/gui-terminal.sh
$G open m06-home 120 40 -- $BIN        # window titled m06-home
$G shot m06-home /tmp/swarm-m0-6/owner/home.png
$G resize m06-home 70 20
$G close m06-home
```

`shot` crops the title bar because it shows the account name. macOS refuses keystroke injection into another process's tty, so a window shows what dustpan drew on start, at the size you set with `open` and `resize`. To show another state, send keys with `agent-desktop --headed press` only with the operator's permission, or take the state from the text lane.

## Evidence

- Save every capture under `/tmp/swarm-m0-6/<role>/` as `<slug>.txt` or `<slug>.png`. Cleanup never deletes that folder.
- Prove the user path: real keys, mouse bytes and resizes into the real binary. Do not call internal functions.
- Prove the action and the state after it: the screen text after each key, `info` for cursor and alternate screen, `stty -a` and the exit code after a quit.
- A PNG goes in the repo only after you look at it for the account name or host name. The repo is public.
- Borders are proven by `box_borders_align` in `home-lanes.py`, which measures cell columns with East Asian widths and fails on a border one cell off its corners. Check it bites by shifting a border in a saved capture.

## Cleanup

```bash
$V stop home                      # each pane you started, by name
tmux -L dustpan-verify -f /dev/null kill-server   # every pane this skill started, nothing else
$G close m06-home                 # each window you opened
```

Never kill by process name. A TERM test kills the child pid you read from `$V pid`.

## Features

`features/README.md` indexes what to cover. Cover every mapped feature, not one convenient entry point.
