#!/usr/bin/env bash
# Completeness gate for docs/inventory. Pulls every subcommand and long flag the
# three Mole dispatchers and command scripts mention, then reports each one the
# inventory never names. Over-inclusive on purpose: flags Mole passes to other
# tools show up too and get triaged into scripts/inventory-gate.ignore.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MAC="${MAC:-$(cd "$ROOT/.." && pwd)/mole}"
INV="${INV:-$ROOT/docs/inventory}"
IGNORE="$ROOT/scripts/inventory-gate.ignore"

extract_branch() {
  local dir
  dir="$(mktemp -d)"
  git -C "$MAC" archive "origin/$1" | tar -x -C "$dir"
  printf '%s' "$dir"
}

sha_of() {
  git -C "$MAC" rev-parse --short "$1" 2>/dev/null || printf 'unknown'
}

if [ -z "${WIN:-}" ]; then WIN="$(extract_branch windows)"; WIN_SHA="$(sha_of origin/windows)"; else WIN_SHA="unknown (WIN set by caller)"; fi
if [ -z "${LNX:-}" ]; then LNX="$(extract_branch linux)"; LNX_SHA="$(sha_of origin/linux)"; else LNX_SHA="unknown (LNX set by caller)"; fi
echo "# sources: main $(sha_of HEAD), windows $WIN_SHA, linux $LNX_SHA"

tokens() {
  { /usr/bin/grep -hoE -- '--[a-z][a-z0-9-]+' "$@" 2>/dev/null || true; } | sort -u
}

go_flags() {
  { /usr/bin/grep -hoE 'flag\.[A-Za-z0-9]+(Var)?\([^"]*"[a-z][a-z0-9-]*"' "$1"/cmd/*/*.go 2>/dev/null || true; } |
    /usr/bin/grep -oE '"[a-z][a-z0-9-]*"' | tr -d '"' | sed 's/^/--/' | sort -u
}

subcommands_sh() {
  { /usr/bin/grep -hoE '^[[:space:]]*"?[a-z][a-z-]+"?( \| "?[a-z-]+"?)*\)' "$1" || true; } |
    tr -d ' "()' | tr '|' '\n' | /usr/bin/grep -v '^-' | sort -u
}

subcommands_ps1() {
  { /usr/bin/grep -hE 'validCommands = @\(' "$1" || true; } |
    /usr/bin/grep -oE '"[a-z]+"' | tr -d '"' | sort -u
}

check() {
  local platform="$1"; shift
  local missing=0
  while IFS= read -r tok; do
    [ -z "$tok" ] && continue
    if [ -f "$IGNORE" ] && /usr/bin/grep -qxF -- "$platform $tok" "$IGNORE"; then continue; fi
    if ! /usr/bin/grep -rqF -- "$tok" "$INV"; then
      echo "MISSING $platform $tok"
      missing=$((missing + 1))
    fi
  done
  echo "# $platform missing: $missing"
}

report="$(
  { tokens "$MAC/mole" "$MAC"/bin/*.sh; go_flags "$MAC"; subcommands_sh "$MAC/mole"; } | sort -u | check macos
  { tokens "$WIN/mole.ps1" "$WIN"/bin/*.ps1; go_flags "$WIN"; subcommands_ps1 "$WIN/mole.ps1"; } | sort -u | check windows
  { tokens "$LNX/mole" "$LNX"/bin/*.sh; go_flags "$LNX"; subcommands_sh "$LNX/mole"; } | sort -u | check linux
)"
printf '%s\n' "$report"
case "$report" in *MISSING*) exit 1 ;; esac
