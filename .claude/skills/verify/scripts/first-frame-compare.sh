#!/usr/bin/env bash
# Interleaved first-frame timing of Mole's menu (`mo`) and dustpan. Prints the mo median first, then dustpan's,
# and exits 1 when dustpan's median is over the budget.
# Usage: first-frame-compare.sh OUTFILE [RUNS=10] [BUDGET_MS=50] [DUSTPAN_BIN]
set -euo pipefail
out=${1:?outfile}
runs=${2:-10}
budget=${3:-50}
here=$(cd "$(dirname "$0")" && pwd)
bin=${4:-$here/../../../../target/release/dustpan}
timer=$here/first-frame.py
mo_times=() dp_times=()
for ((i = 0; i < runs; i++)); do
  mo_times+=("$("$timer" --marker 'Quit' -- mo | head -1)")
  dp_times+=("$("$timer" --marker 'Quit' -- "$bin" | head -1)")
done
median() { printf '%s\n' "$@" | sort -n | awk '{a[NR]=$1} END {print (NR%2) ? a[(NR+1)/2] : (a[NR/2]+a[NR/2+1])/2}'; }
mo_median=$(median "${mo_times[@]}")
dp_median=$(median "${dp_times[@]}")
{
  echo "mo median ${mo_median} ms (runs: ${mo_times[*]})"
  echo "dustpan median ${dp_median} ms (runs: ${dp_times[*]})"
  echo "budget ${budget} ms"
} | tee "$out"
awk -v m="$dp_median" -v b="$budget" 'BEGIN {exit !(m <= b)}' || { echo "FAIL: over budget" | tee -a "$out"; exit 1; }
echo "PASS" | tee -a "$out"
