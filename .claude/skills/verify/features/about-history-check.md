# About pages, History and Check a path

Enter on a Home entry opens a screen. A `soon` entry opens an About page that says what it will do and when. History shows the operations log. Check a path asks the guard about a path. Each screen has its own footer hints and its own `?` help (`F1` on Check). Esc, Left or Backspace goes back to Home with the cursor on the same row, and so does a click on `Home` in the breadcrumb.

## Sub-features

- `about-page` shows the title, the blurb, 2 to 4 lines under `What it will do`, `Arrives in milestone Mn.` and `Until then: Check a path (0) or History (9)`. Keys `0` and `9` open those two screens from the page.
- `history-screen` shows `Recent sessions` (started, command, items, size, status) and `Deletion audit` (when, mode, result, size, path), newest first, then `Log: <path>`. Up, Down, `j`, `k` and the wheel scroll.
- `history-empty` says `Nothing has been cleaned yet.` and `Every removal dustpan makes will show up here.`
- `history-error` shows `The history could not be read.` and the sentence `dustpan history` prints. The screen stays usable.
- `check-field` takes text. `q`, `j`, `k` and digits are letters there. Backspace erases, Ctrl+U clears, Enter checks, Esc goes back.
- `check-verdict` stacks results newest first. Each shows the path, then `Kept  <reason>` or `Eligible  dustpan may clean this when Clean arrives`.
- `check-paste` handles a bracketed paste. A backslash-escaped path (`My\ Folder\ \(1\)`), surrounding quotes and a trailing space are cleaned up, and the field then shows the real path. `~` expands to the home folder. Typed text is never touched: a typed `\` stays in the path.
- `check-many` gives one result for each path in a paste. Lines are separate paths. A line that is not an existing path and splits on unescaped spaces into pieces that all start with `/` or `~` (Terminal.app's multi-file drag) is several paths. Any other line is one path, so `/Users/x/My Folder` stays whole. Results show in paste order, newest paste first, and the field shows several paths with a bullet between them.
- `check-fresh` builds the guard again on each Enter, so an edit to the whitelist shows in the very next check.
- `check-error` wraps an error over several lines, so the cause stays readable at 60, 80 and 100 columns.
- `check-wrap` shows the whole judged path over as many lines as it needs. It is never cut in the middle.
- `read-only` means opening History never creates the log folder, and no screen writes anything.

## How to get to it (user POV)

- On Home press `Enter` on an entry, press its number, or click a row twice.

## Driving it with pty-drive

Preconditions: release binary built. History and Check lanes use a fixture folder as `HOME` and clear `XDG_CONFIG_HOME` and `XDG_STATE_HOME`. Never run them with the real `HOME` and `--debug`, which writes a log.

- **About.** `$V keys home Down Down Enter` shows the Analyze page. `Escape`, `Left`, `BSpace` and `$V click home 12 1` each return to Home with `➤` on `3  Analyze`.
- **History.** Start with `env HOME=<fixture>` and press `9`, `Enter`. `$V wheel home down 10 8` scrolls. Compare a listing of the fixture before and after: it must not change.
- **Check.** `0`, `Enter`, then `$V text home /System` and `Enter` shows `Kept  critical system path`. `$V text home '~/scratch'` shows `Eligible` above it.
- **Paste.** `$V paste home '<fixture>/My\ Folder\ \(1\) '` fills the field. The pty-drive `paste` command wraps the text in bracketed-paste marks because the program turned that mode on.
- **Several paths.** `$V paste home "$H/scratch"$'\n'"$H/keepme"` and `$V paste home "$H/scratch $H/keepme "` each give two results, `scratch` above `keepme`.
- **Whitelist edit.** Check a path, write `~/keepme` into `$H/.config/dustpan/whitelist`, check it again on the same screen: the newest result changes from `Eligible` to `Kept  protected by your whitelist`.
- **Lanes.** `home-lanes.py OUT --only 14,15,16,17,18,19,20,21,22,23` runs About, History, History empty and unreadable, Check, the mouse, a paste of several paths, a whitelist edit, an unreadable whitelist at three widths, the 60 by 16 fit of every help overlay and the Check hint, and the typed backslash. It saves `about.txt`, `history.txt`, `history-empty.txt`, `check.txt`, `mouse.txt`, `check-many.txt`, `check-whitelist.txt`, `check-error.txt`, `small-60x16.txt` and `check-backslash.txt`.

## Gotchas

- A fixture folder under a symlinked path (`/tmp`) makes every check `Kept` because of the symlinked ancestor. Use the `/private/tmp/...` spelling.
- A path longer than the window wraps onto more lines and is never cut. Put the lines back together before comparing, and compare without spaces, because the screen capture drops a trailing space at a wrap. `result_paths` in `home-lanes.py` does both.
- The Check field shows a reversed block where the cursor is. It is not a real terminal cursor, so `$V info` still says `cursor_visible=0`.
