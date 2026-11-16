# About pages, History and Check a path

Enter on a Home entry opens a screen. A `soon` entry opens an About page that says what it will do and when. History shows the operations log. Check a path asks the guard about a path. Each screen has its own footer hints and its own `?` help (`F1` on Check). Esc, Left or Backspace goes back to Home with the cursor on the same row, and so does a click on `Home` in the breadcrumb.

## Sub-features

- `about-page` shows the title, the blurb, 2 to 4 lines under `What it will do`, `Arrives in milestone Mn.` and `Until then: Check a path (0) or History (9)`. Keys `0` and `9` open those two screens from the page.
- `history-screen` shows `Recent sessions` (started, command, items, size, status) and `Deletion audit` (when, mode, result, size, path), newest first, then `Log: <path>`. Up, Down, `j`, `k` and the wheel scroll.
- `history-empty` says `Nothing has been cleaned yet.` and `Every removal dustpan makes will show up here.`
- `history-error` shows `The history could not be read.` and the sentence `dustpan history` prints. The screen stays usable.
- `check-field` takes text. `q`, `j`, `k` and digits are letters there. Backspace erases, Ctrl+U clears, Enter checks, Esc goes back.
- `check-verdict` stacks results newest first. Each shows the path, then `Kept  <reason>` or `Eligible  dustpan may clean this when Clean arrives`.
- `check-paste` handles a bracketed paste. A backslash-escaped path (`My\ Folder\ \(1\)`), surrounding quotes and a trailing space are cleaned up. `~` expands to the home folder.
- `read-only` means opening History never creates the log folder, and no screen writes anything.

## How to get to it (user POV)

- On Home press `Enter` on an entry, press its number, or click a row twice.

## Driving it with pty-drive

Preconditions: release binary built. History and Check lanes use a fixture folder as `HOME` and clear `XDG_CONFIG_HOME` and `XDG_STATE_HOME`. Never run them with the real `HOME` and `--debug`, which writes a log.

- **About.** `$V keys home Down Down Enter` shows the Analyze page. `Escape`, `Left`, `BSpace` and `$V click home 12 1` each return to Home with `➤` on `3  Analyze`.
- **History.** Start with `env HOME=<fixture>` and press `9`, `Enter`. `$V wheel home down 10 8` scrolls. Compare a listing of the fixture before and after: it must not change.
- **Check.** `0`, `Enter`, then `$V text home /System` and `Enter` shows `Kept  critical system path`. `$V text home '~/scratch'` shows `Eligible` above it.
- **Paste.** `$V paste home '<fixture>/My\ Folder\ \(1\) '` fills the field. The pty-drive `paste` command wraps the text in bracketed-paste marks because the program turned that mode on.
- **Lanes.** `home-lanes.py OUT --only 14,15,16,17,18` runs About, History, History empty and unreadable, Check, and the mouse, and saves `about.txt`, `history.txt`, `history-empty.txt`, `check.txt`, `mouse.txt`.

## Gotchas

- A fixture folder under a symlinked path (`/tmp`) makes every check `Kept` because of the symlinked ancestor. Use the `/private/tmp/...` spelling.
- A path longer than the window is cut in the middle with `…`, so assert on the tail of a long path.
- The Check field shows a reversed block where the cursor is. It is not a real terminal cursor, so `$V info` still says `cursor_visible=0`.
