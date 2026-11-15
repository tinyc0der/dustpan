#!/usr/bin/env bash
# Show a program in its own Terminal.app window and screenshot that window. The window shows what the program drew on start;
# keys need focus (agent-desktop --headed press), because macOS refuses TIOCSTI injection from another process.
# Never touches Warp or Ghostty, and closes only windows it opened.
# Usage: gui-terminal.sh <command> NAME [args]
#   open   NAME COLS ROWS -- CMD...   new window titled NAME at COLS x ROWS running CMD, window closes when CMD exits
#   resize NAME COLS ROWS             resize the window with the xterm 8;rows;cols t sequence
#   shot   NAME OUT.png               screenshot the window through agent-desktop, title bar cropped off
#   id     NAME                       agent-desktop window id
#   close  NAME                       close the window if it is still open
set -euo pipefail
cmd=${1:?command}
name=${2:?name}
shift 2
state=${TMPDIR:-/tmp}/dustpan-verify-gui
mkdir -p "$state"
tty_file=$state/$name.tty

window_id() {
  agent-desktop list-windows --app Terminal | python3 -I -c '
import json, sys
wins = [w for w in json.load(sys.stdin).get("data") or [] if sys.argv[1] in w["title"]]
print(wins[0]["id"] if wins else "")
' "$name"
}

case $cmd in
  open)
    cols=$1 rows=$2
    shift 2
    [[ ${1:-} == -- ]] && shift
    rm -f "$tty_file"
    script=$state/$name.command
    {
      printf '#!/bin/bash\n'
      printf 'tty > %q\n' "$tty_file"
      printf "printf '\\\\033[8;%s;%st\\\\033]0;%s\\\\007' %q %q %q\n" "$rows" "$cols" "$name" "$rows" "$cols" "$name"
      printf '%q ' "$@"
      printf '\nexit 0\n'
    } > "$script"
    chmod +x "$script"
    open -a Terminal "$script"
    for _ in $(seq 100); do
      [[ -s $tty_file && -n $(window_id) ]] && break
      sleep 0.1
    done
    [[ -s $tty_file ]] || { echo "window never started" >&2; exit 1; }
    ;;
  resize)
    printf '\033[8;%s;%st' "$2" "$1" > "$(cat "$tty_file")"
    ;;
  shot)
    out=$1
    id=$(window_id)
    [[ -n $id ]] || { echo "no window for $name" >&2; exit 1; }
    agent-desktop screenshot --window-id "$id" "$out" > /dev/null
    height=$(sips -g pixelHeight "$out" | awk '/pixelHeight/ {print $2}')
    width=$(sips -g pixelWidth "$out" | awk '/pixelWidth/ {print $2}')
    sips -c $((height - 64)) "$width" --cropOffset 64 0 "$out" > /dev/null
    ;;
  id) window_id ;;
  close)
    osascript -e 'on run argv' \
      -e 'tell application "Terminal"' \
      -e 'repeat with w in windows' \
      -e 'if (name of w) contains (item 1 of argv) then close w saving no' \
      -e 'end repeat' -e 'end tell' -e 'end run' "$name" > /dev/null 2>&1 || true
    rm -f "$tty_file" "$state/$name.command"
    ;;
  *) echo "unknown command $cmd" >&2; exit 2 ;;
esac
