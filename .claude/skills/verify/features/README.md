# dustpan verification map

This directory is the maintained source for verifying the user-facing behavior of dustpan. Read the index, then use the matching feature file as the recipe.

## Baseline preconditions

- Build once: `cargo build --release -p dustpan`. Set `BIN=$PWD/target/release/dustpan` and `V=.claude/skills/verify/scripts/pty-drive.sh`.
- Drive only panes this run started. `pty-drive.sh` uses its own tmux socket, so nothing else is touched.
- Start each recipe from a fresh pane at 120 by 40 unless the recipe sets another size.
- Home never scans, so no data directory or seed is needed. M0 ships no cleaning, so no recipe can delete anything.

## Driving conventions

- Prefer waiting for text (`$V wait NAME "text" SECONDS`) over sleeping.
- Read the screen with `$V screen NAME`. Read terminal state with `$V info NAME`.
- Save captures under `/tmp/swarm-m0-6/<role>/`.

## Proof and skip reporting

- Capture the key sent and the screen after it.
- A restore proof shows the shell state after exit (`stty -a`, cursor, alternate screen, exit status).
- Report an unreachable path with the command tried and the unmet precondition. A lane the GUI cannot reach (locked screen) is reported as not run, not as passed through the text path.

## Features

| File | Feature |
|---|---|
| `home.md` | Home list, cursor, number keys, toasts, mouse |
| `frame-and-resize.md` | Header, footer hint dropping, detail pane, minimum size |
| `help-overlay.md` | `?` overlay and its border |
| `terminal-restore.md` | Terminal restored on `q`, `Ctrl+C`, SIGTERM and panic |
| `plain-output.md` | `NO_COLOR`, `--ascii`, no TTY |
