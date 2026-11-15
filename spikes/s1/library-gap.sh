#!/usr/bin/env bash
# Usage: library-gap.sh OUTFILE
# Sizes the two ~/Library children that Mole's overview du skips with -I
# (Mobile Documents and CloudStorage), next to the whole of ~/Library.
set -euo pipefail

out=${1:?outfile}
here=$(cd "$(dirname "$0")" && pwd)
bin=${S1WALK:-$here/target/release/s1walk}
raw=$("$bin" bulk "$HOME/Library")
{
  echo "$raw" | awk -F'\t' '$1 == "total" {print "library_total\t" $2}'
  echo "$raw" | awk -F'\t' '$1 == "entry" && ($2 == "Mobile Documents" || $2 == "CloudStorage") {print $2 "\t" $3}'
} > "$out"
cat "$out"
