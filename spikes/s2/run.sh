#!/usr/bin/env bash
# Usage: run.sh OUTDIR [REPS]
# Trashes a file, a folder, a fake .app, an item under a spike-owned container
# folder, and a same-name collision with each of four methods, then removes only
# this run's items from the Trash. The Put Back check is an operator step
# (putback.py).
set -euo pipefail

out=${1:?outdir}
reps=${2:-5}
here=$(cd "$(dirname "$0")" && pwd)
bin=${S2TRASH:-$here/target/release/s2trash}
mkdir -p "$out"
run_id="r$(date +%s)"
work=$(mktemp -d "${TMPDIR:-/tmp}/s2-work.XXXXXX")
container="$HOME/Library/Containers/com.dustpan.spike.s2"
mkdir -p "$container"
tmp_real=$(cd "${TMPDIR:-/tmp}" && pwd -P)
redact() { sed -e "s#$HOME#~#g" -e "s#$tmp_real/*#\$TMPDIR/#g" -e "s#${TMPDIR:-/tmp}/*#\$TMPDIR/#g"; }

{
  echo "date	$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "os	$(sw_vers -productVersion) $(sw_vers -buildVersion)"
  echo "run_id	$run_id"
  echo "reps	$reps"
  echo "session	$(launchctl managername 2>/dev/null)"
  echo "ssh	${SSH_CONNECTION:-none}"
} > "$out/machine.tsv"

perl -e 'alarm 900; exec @ARGV' "$bin" suite "$work" "$container" "$reps" "$run_id" | redact > "$out/suite.tsv"

perl -e 'alarm 300; exec @ARGV' "$bin" cleanup "$run_id" "$work/cleanup" | redact > "$out/cleanup.tsv"
left=$(osascript -e "tell application \"Finder\" to count (items of trash whose name starts with \"s2-$run_id-\")")
echo "left_in_trash	$left" >> "$out/cleanup.tsv"
{ rmdir "$container" 2>&1 || echo "rmdir exit=$?"; } | redact > "$out/container-rmdir.txt"
rm -rf "$work"
python3 -I "$here/summarize.py" "$out" > "$out/summary.tsv"
cat "$out/summary.tsv"
