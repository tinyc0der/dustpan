# S4: terminal glyphs

Spike S4 asks whether sub-cell bars, braille spinners, emoji widths and mouse events render correctly in Terminal.app, iTerm2, Ghostty, Windows Terminal, conhost and a Linux VTE terminal. The answer sets the glyph set and the ASCII fallback rule for the M0-6 TUI frame.

- Branch: `spike/s4-terminal` at `f2a7c6a47eed9e67151cf8b9dcfe44f03b71bd0a`.
- Program: `spikes/s4/src/main.rs` (`s4frame`, ratatui 0.29, crossterm 0.28, unicode-width 0.2).
- Scripts: `spikes/s4/run.sh` opens each terminal on `spikes/s4/show.sh`, captures the window, and merges the width audits with `widths.py`.
- Outputs: `spikes/s4/results/run1/`, `run2/` and `trial-captures/`.

## What the program draws

`s4frame` draws the Clean › Review frame from `docs/PLAN.md` ("Every screen shares one frame") at the terminal's size. The frame has the header with the `⚙ admin` badge, the list with fold marks `▾ ▸`, the `➤` cursor, `[■]` group boxes and an eighth-cell bar per row, the detail pane, a toast line with a braille spinner frame and `✓`, and the footer. A bordered **Glyph check** box in the detail pane prints each glyph group and ends every line with `|` at a column that `unicode-width` computes. If a terminal draws a glyph wider or narrower than `unicode-width` says, that line's `|` moves. `--ascii` draws the same frame with ASCII only.

`s4frame --audit FILE` prints 41 glyphs one at a time at column 1 and reads the cursor column back with a cursor position report (`ESC [6n`). It records the terminal's advance next to the `unicode-width` width.

## Width audit

`show.sh` resized Terminal.app and iTerm2 to 120 by 40 in both runs (`size-*.txt`). Both runs gave identical audits (`run1/widths.tsv` equals `run2/widths.tsv`).

| Glyphs | `unicode-width` | Terminal.app | iTerm2 | Warp (trial) |
|---|---|---|---|---|
| `─ │ ┌ ┐ └ ┘` | 1 | 1 | 1 | 1 |
| `▏ ▎ ▍ ▌ ▋ ▊ ▉ █` | 1 | 1 | 1 | 1 |
| `⠋ ⠙ ⠹ ⠸ ⠼ ⠴ ⠦ ⠧ ⠇ ⠏` | 1 | 1 | 1 | 1 |
| `🧹` (U+1F9F9) | 2 | 2 | 2 | 2 |
| `⚙` (U+2699) | 1 | 1 | 1 | 1 |
| `⚙️` (U+2699 U+FE0F) | 2 | **1** | 2 | **1** |
| `➤ ▾ ▸ ✓ ✗ ■ › · … ← → ○ ◎ ⊙` | 1 | 1 | 1 | 1 |

Forty of the 41 glyphs agree in every terminal measured. The one disagreement is `⚙` followed by the emoji variation selector U+FE0F. `unicode-width` and iTerm2 give it 2 cells. Terminal.app and Warp give it 1. A frame that prints `⚙️` therefore shifts every later cell on that row by one column in Terminal.app and Warp. The bare `⚙` has width 1 everywhere.

The Warp audit and the Terminal.app captures come from a trial of an earlier `run.sh` that captured with `screencapture -l`. The trial Terminal.app audit is identical to `run1/audit-terminal.tsv`. Warp opened the script as a tab in an existing window, so it ran at 152 by 59 (`trial-captures/size-warp.txt`) and only once.

## Screenshots

`trial-captures/` holds the Unicode and ASCII frames from Terminal.app (120 by 40) and from Warp. `docs/spikes/media/M0-2-review-s4-terminal.png` is a copy of the Terminal.app Unicode frame. Terminal.app captures drop the title bar, which shows the account name. The Warp captures are cropped to the terminal pane. In both terminals, the box borders join, the eighth-cell bars grow in steps inside one cell, the spinner frame and `✓` sit in their cells, and `🧹` takes two cells without moving the `|` column in the **Glyph check** box. In Warp the partial blocks render thinner than in Terminal.app, but they stay in their cells.

The two final runs captured no screenshots (`captures.tsv` says `no-window`). The screen was locked during both runs, and `screencapture` could not create an image from a window. `agent-desktop screenshot` also failed, with "Global application window inventory did not stabilize". Two Ghostty processes that this spike started with `open -na Ghostty --args -e …` had no windows and did not answer, which blocked that inventory. Ghostty never ran the script, and an AppleScript `new window` call to it timed out.

## ASCII variant

The ASCII frame replaces box drawing with `+ - |`, the bars with `#` and `:`, the spinner with `| / - \`, `▾ ▸ ➤` with `v > >`, `✓ ✗` with `+ x`, `🧹` with `~`, and `⚙ admin` with `* admin`. Each replacement keeps the cell count of the glyph it replaces, except `🧹` (2 cells) becomes `~` (1 cell), which `pad` absorbs. The trial captures show one gap. The footer still prints `←→ Fold`, because the spike did not map the footer arrows. The M0-6 glyph table must cover every non-ASCII character in the frame, including arrows.

## Mouse events

This spike did not test mouse events. They need a click inside each terminal, which needs physical input.

## Pending operator runs

Do these in one session with the screen unlocked and nobody using the keyboard.

1. Quit the two stray Ghostty processes from this spike. Their command lines start with `/Applications/Ghostty.app/Contents/MacOS/ghostty --window-width=120` and `/Applications/Ghostty.app/Contents/MacOS/ghostty -e`.
2. Run `spikes/s4/run.sh spikes/s4/results/run3 terminal iterm ghostty`. Check that `captures.tsv` says `captured` for six frames. Copy the Terminal.app and Ghostty frames to `docs/spikes/media/M0-2-review-s4-macos.png`.
3. On Windows, run the frame in Windows Terminal and in conhost and save `s4-windows.png`. The bars and borders must align, or the ASCII fallback must apply.
4. On Linux, run it in a VTE terminal (GNOME Terminal) and save a screenshot.
5. In each terminal, click a list row and scroll the wheel once. Check that no escape text appears in the frame.

A Ghostty, Windows or VTE result changes the verdict only if a glyph other than `⚙️` measures a width different from `unicode-width`. In that case, add the glyph to the avoid list or move that terminal to the ASCII fallback.

Verdict. Use box drawing `─│┌┐└┘`, eighth blocks `▏▎▍▌▋▊▉█`, braille spinner frames `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏`, `🧹`, bare `⚙` (U+2699), `➤ ▾ ▸ ✓ ✗ ■ › ·`, and never `⚙️` with U+FE0F, which Terminal.app and Warp draw one cell narrower than `unicode-width`; fall back to the ASCII table on `--ascii` or a non-UTF-8 locale, with every frame character including arrows mapped to an ASCII string of the same cell width.
