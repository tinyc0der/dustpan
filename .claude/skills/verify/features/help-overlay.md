# Help overlay

`?` opens a bordered box over the screen that lists every key on Home and each entry's jump key. `?`, `Esc` or `q` closes it.

## Sub-features

- `help-open` lists every Home key.
- `help-border` draws the box with its right border in the column of its right corners on every row, with the wide broom glyph in the title.
- `help-close` closes on `?`, `Esc` and `q`, and `q` does not quit.

## How to get to it (user POV)

- Press `?` on Home.

## Driving it with pty-drive

Preconditions: pane `home` at 120 by 40.

- **Open.** `$V keys home '?'` then `$V screen home`. The box title reads `🧹 Help for Home`. Rows show `↑ ↓ j k`, `Enter → l`, `1..9`, `,`, `?`, `q`, `Ctrl+C`, `Wheel, click`, and `9 History`.
- **Borders.** Run `python3 .claude/skills/verify/scripts/home-lanes.py OUT --only 3,4`. Both lanes pass `box_borders_align`.
- **Close.** `$V keys home q`. The box is gone and the pane is still running (`$V info home` says `dead=0`).

## Gotchas

- Keys other than `?`, `Esc`, `q` and `Ctrl+C` do nothing while help is open. A moving cursor behind it is a bug.
- At 60 by 16 the box fills the height. Check the last jump-key row is not cut.
