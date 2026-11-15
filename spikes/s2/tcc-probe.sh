#!/usr/bin/env bash
# Usage: tcc-probe.sh OUTFILE
# Records which Trash operations privacy protection (TCC) allows this process.
set -u

out=${1:?outfile}
probe=$(mktemp "${TMPDIR:-/tmp}/s2-tcc.XXXXXX")
name=$(basename "$probe")
{
  echo "date	$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "session	$(launchctl managername 2>/dev/null)"
  ls "$HOME/.Trash" > /dev/null 2>&1 && echo "list ~/.Trash	allowed" || echo "list ~/.Trash	denied"
  head -c 1 "$HOME/.Trash/.DS_Store" > /dev/null 2>&1 && echo "read ~/.Trash/.DS_Store	allowed" || echo "read ~/.Trash/.DS_Store	denied"
  mv "$probe" "$HOME/.Trash/$name" 2> /dev/null && echo "rename into ~/.Trash	allowed" || echo "rename into ~/.Trash	denied"
  stat -f '%z' "$HOME/.Trash/$name" > /dev/null 2>&1 && echo "stat ~/.Trash/<known name>	allowed" || echo "stat ~/.Trash/<known name>	denied"
  rm "$HOME/.Trash/$name" 2> /dev/null && echo "unlink ~/.Trash/<known name>	allowed" || echo "unlink ~/.Trash/<known name>	denied"
} > "$out"
rm -f "$probe"
cat "$out"
