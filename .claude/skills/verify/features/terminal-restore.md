# Terminal restore

The terminal is put back on every way out: `q`, `Ctrl+C`, a panic and SIGTERM. Raw mode is off, the cursor is shown, the alternate screen is left and mouse capture is off.

## Sub-features

- `restore-q` after `q`, exit 0.
- `restore-ctrl-c` after `Ctrl+C`, exit 130.
- `restore-sigterm` after SIGTERM, exit 130.
- `restore-hangup` ends the process when its terminal closes, with or without SIGHUP, and never spins (lanes 11 and 13).
- `restore-panic` after a panic, with the panic message readable.

## How to get to it (user POV)

- Quit dustpan, or end its process, from a shell.

## Driving it with pty-drive

Preconditions: a shell pane, `$V start sh 120 40`, with the binary started in it.

- **Quit.** `$V keys sh C-c`, then `$V text sh 'echo exit=$?; stty -a'` and `$V keys sh Enter`. The output has `exit=130`, `icanon`, `echo`, `isig` without a leading dash, and `$V info sh` says `cursor_visible=1 alternate_screen=0`.
- **SIGTERM.** Read the child with `pgrep -P "$($V pid sh)"`, send `kill -TERM <pid>`, then run the same checks.
- **Hangup.** `P=$($V pid home); $V stop home; sleep 1; ps -p $P`. The pid is gone. Lane 11 does this.
- **Panic.** Build a throwaway copy with `panic!()` on a key in `update_home`, run it in a shell pane, press the key. The shell shows the panic message in cooked mode and the same checks pass. Revert the copy; never commit it.
- **Lane runner.** `home-lanes.py OUT --only 6,7` does the first two.

## Gotchas

- Read `stty -a` in the same shell that ran dustpan. A new pane proves nothing.
- A pane with `remain-on-exit` shows `dead=1` once its program exits. The shell pane stays alive.
