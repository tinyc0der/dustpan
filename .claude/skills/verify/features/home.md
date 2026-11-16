# Home

Bare `dustpan` in a terminal opens Home. A welcome line sits at the top of the list. Ten entries follow: Clean, Uninstall, Analyze, Optimize, Status, a divider, then Purge, Installers, Slim, History and Check a path. Each row shows its key, its label, a one-line description and a tag. History and Check a path are `ready`. The rest are `soon`, and Enter on one opens its About page (`about-history-check.md`).

## Sub-features

- `home-list` shows the ten entries in order with a divider after Status, each with its blurb and tag.
- `home-nav` moves the cursor with arrows, `j`, `k`, number keys `0` to `9`, the wheel and a click.
- `home-open` opens the entry on Enter, `l`, `Right`, or a click on the already selected row.
- `home-settings` shows the toast "Settings: arrives in M3" on `,`.
- `home-header` shows the boot volume name and free space.
- `home-detail` shows the blurb, `Ready` or `Arrives in Mn`, the key and a hint at 100 columns or more, or after Tab below that.

## How to get to it (user POV)

- Run `dustpan` in a terminal at least 60 columns by 16 rows.

## Driving it with pty-drive

Preconditions: release binary built, no pane named `home`.

- **Open.** `$V start home 120 40 -- $BIN` then `$V wait home "q Quit" 5`. The screen shows the welcome line, the ten entries and a first row ending in `GB free`.
- **Navigate.** `$V keys home Down Down Down Down k`. The only row with `➤` is `4  Optimize`.
- **Jump.** `$V keys home 9`. The `➤` is on `9  History`. `$V keys home 0` puts it on `0  Check a path`.
- **Open an entry.** `$V keys home Enter` on a `soon` row opens its About page, and `Escape` returns to the same row.
- **Settings.** `$V keys home ,`. The row above the footer rule reads `Settings: arrives in M3`. The next key clears it.
- **Wheel.** `$V wheel home down 10 5` then `$V screen home`. The cursor moved down one entry and no escape text appears.
- **Click.** `$V click home 10 6`. The `➤` is on the clicked row (row 6 is `2  Uninstall` at 120 by 40) and the screen shows no stray characters. A second click on the same row opens it.
- **Proof.** Save `$V screen home` after each step. `home-lanes.py` lanes 1, 2, 4, 12 and 18 do all of this.

## Gotchas

- The list starts with the welcome row, then a blank row, then the entries, and the divider takes a row, so click rows are not entry numbers. At 60 by 16 the blank row and the divider are dropped so all ten entries fit.
- The detail pane exists only at 100 columns or more.
- Nothing on Home deletes, moves or writes anything. Enter opens a screen, and the screens only read.
