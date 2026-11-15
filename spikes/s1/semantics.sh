#!/usr/bin/env bash
# Usage: semantics.sh ROOT OUTFILE
# Runs the getattrlistbulk walk once with du semantics and once with Mole's
# file-size rules (min of allocated and logical size, APFS clones that share
# every block counted once), and prints the totals and the two largest rows.
set -euo pipefail

root=${1:?root}
out=${2:?outfile}
here=$(cd "$(dirname "$0")" && pwd)
bin=${S1WALK:-$here/target/release/s1walk}
{
  for m in bulk bulk-mole; do
    "$bin" "$m" "$root" | awk -F'\t' -v m="$m" '
      $1 == "wall_ms" || $1 == "total" || $1 == "errors" { print m "\t" $1 "\t" $2 }
      $1 == "entry" && ($2 == "Library" || $2 == "Projects") { print m "\tentry " $2 "\t" $3 }'
  done
} > "$out"
cat "$out"
