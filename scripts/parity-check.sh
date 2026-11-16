#!/usr/bin/env bash
# Usage: scripts/parity-check.sh <clean|purge|installer> <parity-output-file>
# Fails when the saved output of `scripts/parity.sh <command>` on the standard
# fixture lacks the lists catalog PRs rely on.
set -euo pipefail
which_cmd=$1
out=$2

fail() { echo "parity-check ($which_cmd): $*" >&2; exit 1; }
section() { awk -v t="== $1 (" 'index($0, t) == 1 { on = 1; next } /^== / { on = 0 } on' "$out"; }

grep -q '^fixture untouched: yes (0 modified, 0 removed, 0 added)' "$out" || fail "the fixture changed during the run"
grep -q '^== only Mole found ([1-9][0-9]*) ==' "$out" || fail "Mole's list is empty"
if section 'only dustpan found' | grep -q '^  /'; then fail "an absolute path in the dustpan list"; fi

mole=$(section 'only Mole found')
has() { grep -qxF "  $1" <<<"$mole" || fail "missing $1"; }
lacks() { ! grep -qxF "  $1" <<<"$mole" || fail "unexpected $1"; }
case $which_cmd in
  clean)
    has '~/Library/Caches/Google'
    has '~/Library/Logs/ExampleApp'
    lacks '~/Library/Caches/com.example.notes'
    lacks '~/Documents/notes.txt'
    ;;
  purge)
    has '~/Projects/oldweb/node_modules'
    has '~/Projects/oldrust/target'
    lacks '~/Projects/freshweb/node_modules'
    ;;
  installer)
    has '~/Downloads/Example-1.0.dmg'
    has '~/Downloads/ExampleInstaller.zip'
    lacks '~/Downloads/photos.zip'
    ;;
  *) fail "unknown command" ;;
esac
echo "parity-check ($which_cmd): ok"
