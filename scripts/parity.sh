#!/usr/bin/env bash
# Usage: scripts/parity.sh <clean|purge|installer|self-check> [--manifest FILE] [--whitelist PATH] [--keep] [--save DIR]
# Prints the paths only Mole found, only dustpan found, and both found, from a
# sandboxed `mo <command> --dry-run` on a fresh fixture home. See tests/parity.
set -euo pipefail
cd "$(dirname "$0")/.."
exec cargo run --quiet -p dustpan-parity --bin parity -- "$@"
