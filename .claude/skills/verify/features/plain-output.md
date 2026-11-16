# Plain output

`NO_COLOR` or `--no-color` turns color off. `--ascii` or a non-UTF-8 locale draws only ASCII. With no TTY on stdin or stdout, dustpan prints `--help` and draws nothing.

## Sub-features

- `plain-no-color` emits no foreground or background color codes and keeps the `➤` mark and "soon" and "ready" tags and the "Arrives in milestone Mn" page words.
- `plain-ascii` emits no non-ASCII character on Home, help or the too-small screen.
- `plain-locale` falls back to ASCII when `LC_ALL=C`.
- `plain-no-tty` prints help, no escape codes, exit 0.

## How to get to it (user POV)

- Run `NO_COLOR=1 dustpan`, `dustpan --ascii`, `LC_ALL=C dustpan`, or `dustpan </dev/null | cat`.

## Driving it with pty-drive

Preconditions: release binary built.

- **No color.** `$V start p 120 40 -- env NO_COLOR=1 $BIN`, then `tmux -L dustpan-verify -f /dev/null capture-pane -p -e -t p`. No `38;` `48;` `3x` or `4x` SGR codes appear.
- **ASCII.** `$V start p 120 40 -- $BIN --ascii`, open help with `?`. `$V screen p | LC_ALL=C grep -nP '[^\x00-\x7F]'` prints nothing.
- **Locale.** `$V start p 120 40 -- env LC_ALL=C $BIN`. The cursor row starts with `>` and the border uses `+ - |`.
- **No TTY.** `$BIN </dev/null | cat` prints `Usage:` and exits 0.
- **Lane runner.** `home-lanes.py OUT --only 8,10`.

## Gotchas

- `--no-color` also quiets clap's help colors. Check help text separately from Home.
- A reversed row is a style, not a color. It is allowed under `NO_COLOR`, and the `➤` mark backs it up.
