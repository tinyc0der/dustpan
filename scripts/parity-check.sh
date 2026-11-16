#!/usr/bin/env bash
# Usage: scripts/parity-check.sh <clean|purge|installer> <parity-output-file>
# Fails unless the "only Mole found" list in the saved output of
# `scripts/parity.sh <command>` on the standard fixture equals, row for row,
# tests/parity/expected/<command>.txt (verified against Mole 1.67.0). A
# truncated list and an extra row both fail.
set -euo pipefail
which_cmd=$1
out=$2
here=$(cd "$(dirname "$0")/.." && pwd)
expected="$here/tests/parity/expected/$which_cmd.txt"

fail() { echo "parity-check ($which_cmd): $*" >&2; exit 1; }
section() { awk -v t="== $1 (" 'index($0, t) == 1 { on = 1; next } /^== / { on = 0 } on' "$out"; }

[[ -f "$expected" ]] || fail "no expected list at $expected"
grep -q '^fixture untouched: yes (0 modified, 0 removed, 0 added)' "$out" || fail "the fixture changed during the run"
if section 'only dustpan found' | grep -q '^  /'; then fail "an absolute path in the dustpan list"; fi

got=$(section 'only Mole found' | sed 's/^  //' | LC_ALL=C sort)
want=$(LC_ALL=C sort "$expected")
if [[ "$got" != "$want" ]]; then
  echo "parity-check ($which_cmd): Mole's list differs from $expected" >&2
  diff <(echo "$want") <(echo "$got") >&2 || true
  exit 1
fi
echo "parity-check ($which_cmd): ok ($(wc -l < "$expected" | tr -d ' ') rows)"
