#!/usr/bin/env bash
# Usage: run.sh OUTDIR
# Unattended part of S3. It needs no input and never calls sudo.
set -euo pipefail

out=${1:?outdir}
here=$(cd "$(dirname "$0")" && pwd)
bin=${S3PRIV:-$here/target/release/s3priv}
mkdir -p "$out"
python3 -I "$here/harness.py" "$bin" "$out" > "$out/summary.txt"
cat "$out/summary.txt"
