#!/usr/bin/env bash
# Usage: run.sh ROOT RUNS OUTDIR
# Mole's analyzer cache is moved aside for the whole script and wiped before
# every Mole run, then the original is put back.
set -euo pipefail

root=${1:?root}
runs=${2:-5}
out=${3:?outdir}
here=$(cd "$(dirname "$0")" && pwd)
bin=${S1WALK:-$here/target/release/s1walk}
cache="$HOME/.cache/mole/analyzer"
backup="${TMPDIR:-/tmp}/s1-mole-analyzer-backup.$$"

mkdir -p "$out"
if [[ -e "$cache" ]]; then
  mv "$cache" "$backup"
fi
restore() {
  rm -rf "$cache"
  if [[ -e "$backup" ]]; then
    mv "$backup" "$cache"
  fi
}
trap restore EXIT

{
  echo "date	$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "os	$(sw_vers -productVersion) $(sw_vers -buildVersion)"
  echo "cpu	$(sysctl -n machdep.cpu.brand_string) ncpu=$(sysctl -n hw.ncpu)"
  echo "mole	$(mo --version 2>/dev/null | grep -m1 .)"
  echo "root	$root"
  echo "runs	$runs"
} > "$out/machine.tsv"

now() { perl -MTime::HiRes=time -e 'printf "%.3f\n", time'; }
methods=(mole du jwalk bulk)
echo "method	run	wall_s	exit" > "$out/times.tsv"
for ((i = 1; i <= runs; i++)); do
  shift_by=$(((i - 1) % ${#methods[@]}))
  rotated=("${methods[@]:shift_by}" "${methods[@]:0:shift_by}")
  for m in "${rotated[@]}"; do
    rc=0
    if [[ $m == mole ]]; then
      rm -rf "$cache"
      t0=$(now)
      mo analyze --json "$root" > "$out/raw-mole-$i.json" 2> "$out/raw-mole-$i.err" < /dev/null || rc=$?
      t1=$(now)
    else
      t0=$(now)
      "$bin" "$m" "$root" > "$out/raw-$m-$i.tsv" 2> "$out/raw-$m-$i.err" || rc=$?
      t1=$(now)
    fi
    wall=$(echo "$t1 - $t0" | bc)
    echo "$m	$i	$wall	$rc" | tee -a "$out/times.tsv"
  done
done
