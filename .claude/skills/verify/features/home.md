# Home

Bare `dustpan` in a terminal opens Home. It lists Clean, Uninstall, Analyze, Optimize, Status, a divider, then Purge, Installers, Slim and History. Entries for later milestones say "Coming in Mn" and do not open.

## Sub-features

- `home-list` shows the nine entries in order with a divider after Status.
- `home-nav` moves the cursor with arrows, `j`, `k`, number keys, the wheel and a click.
- `home-toast` shows "<label>: Coming in Mn" on Enter and "Settings: Coming in M3" on `,`.
- `home-header` shows the boot volume name and free space.

## How to get to it (user POV)

- Run `dustpan` in a terminal at least 60 columns by 16 rows.

## Driving it with pty-drive

Preconditions: release binary built, no pane named `home`.

- **Open.** `$V start home 120 40 -- $BIN` then `$V wait home "q Quit" 5`. The screen shows the nine entries and a first row ending in `GB free`.
- **Navigate.** `$V keys home Down Down Down Down k`. The only row with `➤` is `4  Optimize`.
- **Jump.** `$V keys home 9`. The `➤` is on `9  History`.
- **Toast.** `$V keys home Enter`. The row above the footer rule reads `History: Coming in M3`. The next key clears it.
- **Settings.** `$V keys home ,`. The toast reads `Settings: Coming in M3`.
- **Wheel.** `$V wheel home down 10 5` then `$V screen home`. The cursor moved down one entry and no escape text appears.
- **Click.** `$V click home 10 7`. The `➤` is on the clicked row (row 7 is `4  Optimize` at this size) and the screen shows no stray characters.
- **Proof.** Save `$V screen home` after each step.

## Gotchas

- The list starts one row below the rule, and the divider takes a row, so click rows are not entry numbers.
- Enter never opens anything in M0. A screen change after Enter is a bug.
- The detail pane exists only at 100 columns or more.
