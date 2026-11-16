#!/bin/bash
# usage: scripts/open-handles-check.sh [DUSTPAN_BINARY] [OUT_DIR]
#
# macOS only, and it reads the live machine: the result depends on what runs right now.
# Lists every `~/Library/Caches/<name>` and `~/Library/Group Containers/<name>/Library/Caches`
# folder in which a process of this user holds a file open, as `lsof` sees it, and asks
# `dustpan explain` about each. Exits 1 when any of them reads `Eligible`, when no folder is
# held, and when the binary does not run.
set -u
bin=${1:-target/release/dustpan}
"$bin" --version > /dev/null || exit 1
out=${2:-$(mktemp -d)}
mkdir -p "$out"

lsof -n -u "$(id -u)" -F pn 2>/dev/null | python3 -I -c '
import os, re, sys
home = re.escape(os.path.expanduser("~"))
scopes = [
    re.compile(home + r"/Library/Caches/[^/]+"),
    re.compile(home + r"/Library/Group Containers/[^/]+/Library/Caches"),
]
held = set()
for line in sys.stdin:
    if line.startswith("n"):
        for scope in scopes:
            found = scope.match(line[1:].rstrip("\n"))
            if found:
                held.add(found.group(0))
print("\n".join(sorted(held)))
' > "$out/held-folders.txt"

tr '\n' '\0' < "$out/held-folders.txt" | xargs -0 "$bin" explain > "$out/explain.txt" 2>&1
folders=$(grep -c . "$out/held-folders.txt")
eligible=$(grep -c '  Eligible$' "$out/explain.txt")
echo "held folders: $folders, read Eligible: $eligible (details in $out)"
grep '  Eligible$' "$out/explain.txt"
[ "$folders" -gt 0 ] && [ "$eligible" -eq 0 ]
