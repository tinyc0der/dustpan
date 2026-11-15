# Frame and resize

Every screen shares a header, body, toast line and footer. The footer drops hints by priority as the width shrinks. At 100 columns or more a detail pane sits beside the list. Below 60 by 16 a minimum-size message waits for a resize.

## Sub-features

- `frame-header` shows `dustpan › Home` and the volume, dropping the volume name before the free space.
- `frame-hints` drops `Ctrl+C Quit now` first and keeps `q Quit` and `? Help` last.
- `frame-detail` shows the detail pane at 100 columns or more.
- `frame-too-small` shows "dustpan needs 60 by 16" and returns to Home after a resize.

## How to get to it (user POV)

- Resize the terminal window while dustpan runs.

## Driving it with pty-drive

Preconditions: pane `home` running at 120 by 40.

- **Narrow.** `$V resize home 70 20` then `$V screen home`. No row is wider than 70 and the footer lacks `Ctrl+C` but ends `? Help  q Quit`.
- **Smallest.** `$V resize home 60 16`. Home still draws, nothing wraps.
- **Too small.** `$V resize home 50 12`. The screen reads `dustpan needs 60 by 16` and `Resize to continue`.
- **Return.** `$V resize home 120 40`. Home and the detail pane are back, with the cursor where it was.
- **Quit while small.** `$V resize home 50 12` then `$V keys home q`. The program exits.

## Gotchas

- tmux `resize-window` sends a real SIGWINCH, so it matches a window resize. A terminal that loses the resize event is covered by the per-frame size check, which this recipe cannot trigger.
