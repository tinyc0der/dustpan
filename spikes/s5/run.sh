#!/usr/bin/env bash
# Usage: run.sh OUTDIR
# Runs mo clean/purge/installer in dry-run mode against a fixture HOME inside a
# sandbox that denies writes outside temp dirs, and records proof that the real
# home's Mole state did not change.
set -euo pipefail

out=${1:?outdir}
here=$(cd "$(dirname "$0")" && pwd)
profile="$here/deny-writes.sb"
mkdir -p "$out"
out=$(cd "$out" && pwd)
fixture=$(mktemp -d "${TMPDIR:-/tmp}/s5-fixture.XXXXXX")
fixture=$(cd "$fixture" && pwd -P)
fhome="$fixture/home"
real_home=$HOME

redact() { sed -e "s#$fixture#\$FIXTURE#g" -e "s#$real_home#~#g"; }
in_sandbox() { sandbox-exec -f "$profile" "$@"; }

"$here/mkfixture.sh" "$fixture"

{
  echo "date	$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "os	$(sw_vers -productVersion) $(sw_vers -buildVersion)"
  echo "mole	$(mo --version 2>/dev/null | grep -m1 .)"
  echo "mo	$(readlink -f "$(command -v mo)")"
  echo "host env names passed to mo	$(env | grep -E '^(ANDROID|DOCKER|JAVA_HOME|GOPATH|DENO)' | cut -d= -f1 | sort | tr '\n' ' ')"
} | redact > "$out/machine.tsv"

probe="$real_home/.s5-sandbox-probe.$$"
{
  set +e
  in_sandbox /bin/sh -c "echo probe > '$probe'" 2>&1
  echo "write under /Users exit=$?"
  [[ -e $probe ]] && echo "probe file EXISTS" && rm -f "$probe" || echo "probe file absent"
  in_sandbox /usr/bin/sudo -n true 2>&1
  echo "exec /usr/bin/sudo exit=$?"
  in_sandbox /bin/sh -c "echo ok > '$fixture/probe' && cat '$fixture/probe' && rm '$fixture/probe'" 2>&1
  echo "write under fixture exit=$?"
  set -e
} | redact > "$out/sandbox-proof.txt"

snap_real() {
  for d in "$real_home/.config/mole" "$real_home/Library/Logs/mole" "$real_home/.cache/mole"; do
    if [[ -e $d ]]; then find "$d" -exec stat -f '%m %z %N' {} +; else echo "absent $d"; fi
  done | sort
}
snap_fixture() {
  find "$fixture" \( -path "$fhome/.config/mole" -o -path "$fhome/Library/Logs/mole" -o -path "$fhome/.cache/mole" \) -prune -o -exec stat -f '%m %z %N' {} + | sort
}

snap_fixture > "$out/.fixture-before"
echo "command	exit	wall_s	real_home_changed" > "$out/runs.tsv"
run_mo() {
  local name=$1
  shift
  snap_real > "$out/.real-before"
  local t0 t1 rc=0
  t0=$(perl -MTime::HiRes=time -e 'printf "%.3f", time')
  env HOME="$fhome" perl -e 'alarm shift; exec @ARGV' 600 \
    sandbox-exec -f "$profile" mo "$@" < /dev/null > "$out/.raw-$name.stdout" 2> "$out/.raw-$name.stderr" || rc=$?
  t1=$(perl -MTime::HiRes=time -e 'printf "%.3f", time')
  snap_real > "$out/.real-after"
  local changed=no
  diff "$out/.real-before" "$out/.real-after" > "$out/.real-diff" || changed=yes
  redact < "$out/.real-before" > "$out/real-home-$name-before.txt"
  redact < "$out/.real-diff" > "$out/real-home-$name-diff.txt"
  redact < "$out/.raw-$name.stdout" > "$out/$name.stdout"
  redact < "$out/.raw-$name.stderr" > "$out/$name.stderr"
  echo "$name	$rc	$(echo "$t1 - $t0" | bc)	$changed" >> "$out/runs.tsv"
}

run_mo clean clean --dry-run
run_mo purge purge --dry-run --yes
run_mo installer installer --dry-run

snap_fixture > "$out/.fixture-after"
diff "$out/.fixture-before" "$out/.fixture-after" | redact > "$out/fixture-diff.txt" || true
mkdir -p "$out/mole-state"
for f in .config/mole/clean-list.txt .config/mole/purge_paths Library/Logs/mole/operations.log Library/Logs/mole/deletions.log; do
  if [[ -f "$fhome/$f" ]]; then
    redact < "$fhome/$f" > "$out/mole-state/$(echo "$f" | tr '/' '_')"
  fi
done
(cd "$fhome" && find . \( -path ./.config/mole -o -path ./Library/Logs/mole -o -path ./.cache/mole \) -print) | sort > "$out/mole-state/dirs-created.txt"
python3 -I "$here/parse.py" "$out" > "$out/candidates.tsv"
rm -f "$out"/.raw-* "$out"/.real-* "$out"/.fixture-*
rm -rf "$fixture"
cat "$out/runs.tsv"
