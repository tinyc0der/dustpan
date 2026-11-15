#!/usr/bin/env bash
# Usage: run.sh OUTDIR [TERMINAL...]
# Opens each terminal on show.sh, captures only that terminal's window for the
# Unicode and ASCII frames, and collects the glyph-width audit it writes.
# Terminal.app captures drop the title bar, which shows the account name.
set -uo pipefail

out=${1:?outdir}
shift
terms=("${@:-terminal iterm ghostty warp}")
read -r -a terms <<< "${terms[*]}"
here=$(cd "$(dirname "$0")" && pwd)
bin=${S4FRAME:-$here/target/release/s4frame}
mkdir -p "$out"
out=$(cd "$out" && pwd)
work=$(mktemp -d "${TMPDIR:-/tmp}/s4-work.XXXXXX")

wait_for() {
  local f=$1 limit=$2
  for ((i = 0; i < limit * 10; i++)); do
    [[ -e $f ]] && return 0
    perl -e 'select undef, undef, undef, 0.1'
  done
  return 1
}

newest_window() {
  agent-desktop list-windows --app "$1" | python3 -I -c '
import json, sys
wins = [w for w in json.load(sys.stdin).get("data") or [] if sys.argv[1] in w["title"]]
print(max(wins, key=lambda w: int(w["id"].split("-")[1]))["id"] if wins else "")
' "$2"
}

capture() {
  local tag=$1 variant=$2
  local app id png="$out/s4-$tag-$variant.png"
  case $tag in
    terminal) app=Terminal ;;
    iterm) app=iTerm2 ;;
    ghostty) app=Ghostty ;;
    warp) app=Warp ;;
  esac
  local tool=agent-desktop
  id=$(newest_window "$app" "dustpan-s4-$tag")
  if [[ -n $id ]]; then
    agent-desktop screenshot --window-id "$id" "$png" > /dev/null
  fi
  if [[ ! -s $png ]]; then
    tool=screencapture
    id=$(osascript -l JavaScript "$here/winid.js" "dustpan-s4-$tag")
    [[ -n $id ]] && screencapture -x -o -l "$id" "$png"
  fi
  if [[ ! -s $png ]]; then
    echo "$tag	$variant	no-window"
    return
  fi
  if [[ $tag == terminal ]]; then
    local h w
    h=$(sips -g pixelHeight "$png" | awk '/pixelHeight/ {print $2}')
    w=$(sips -g pixelWidth "$png" | awk '/pixelWidth/ {print $2}')
    sips -c $((h - 64)) $((w - 1)) --cropOffset 64 1 "$png" > /dev/null
  fi
  echo "$tag	$variant	captured by $tool	$(sips -g pixelWidth -g pixelHeight "$png" | awk '/pixel/ {printf "%s ", $2}')"
}

launch() {
  local tag=$1
  local cmd="$work/s4-$tag.command"
  printf '#!/bin/bash\n%q %q %q %q\nexit 0\n' "$here/show.sh" "$bin" "$out" "$tag" > "$cmd"
  chmod +x "$cmd"
  case $tag in
    terminal) open -a Terminal "$cmd" ;;
    iterm)
      perl -e 'alarm 20; exec @ARGV' osascript -e 'on run argv' \
        -e 'tell application "iTerm2"' -e 'set w to (create window with default profile command (item 1 of argv))' \
        -e 'tell current session of w to set columns to 120' -e 'tell current session of w to set rows to 40' -e 'end tell' \
        -e 'end run' "$cmd" > /dev/null
      ;;
    warp) open -a Warp "$cmd" ;;
    ghostty)
      perl -e 'alarm 20; exec @ARGV' osascript -e 'on run argv' -e 'tell application "Ghostty"' \
        -e 'set cfg to new surface configuration' -e 'set command of cfg to (item 1 of argv)' \
        -e 'new window with configuration cfg' -e 'end tell' -e 'end run' "$cmd" > /dev/null
      ;;
  esac
}

echo "terminal	variant	result	pixels" > "$out/captures.tsv"
for tag in "${terms[@]}"; do
  rm -f "$out/.ready-$tag-"* "$out/.done-$tag"
  launch "$tag"
  for variant in unicode ascii; do
    if wait_for "$out/.ready-$tag-$variant" 40; then
      capture "$tag" "$variant" >> "$out/captures.tsv"
    else
      echo "$tag	$variant	timeout" >> "$out/captures.tsv"
    fi
  done
  wait_for "$out/.done-$tag" 20 || true
done
rm -f "$out"/.ready-* "$out"/.done-*
rm -rf "$work"
python3 -I "$here/widths.py" "$out" > "$out/widths.tsv"
cat "$out/captures.tsv" "$out/widths.tsv"
