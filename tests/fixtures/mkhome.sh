#!/bin/bash
# Builds a fixture home folder for tests, live lanes and the guard bench.
#
#   tests/fixtures/mkhome.sh DIR
#
# Run a command against it with HOME=DIR. Running it again on the same DIR changes nothing.
set -euo pipefail

dir=${1:?usage: mkhome.sh DIR}
case "$dir" in
    / | "${HOME:-/}" | "${HOME:-/}/")
        echo "mkhome.sh: refusing to build a fixture in $dir" >&2
        exit 2
        ;;
esac

mkdir -p "$dir"
cd "$dir"

file() {
    mkdir -p "$(dirname "$1")"
    printf '%s\n' "${2:-fixture}" > "$1"
}

# Caches named by bundle id: two plain apps and one Apple app the guard keeps by name.
file Library/Caches/com.example.app/blob.bin
file Library/Caches/com.example.app/index.json '{}'
file Library/Caches/com.example.editor/cache.bin
file Library/Caches/com.apple.Safari/Cache.data
file Library/Preferences/com.example.app.plist

# Head folders the guard keeps, each with a file it does not.
file Documents/report.txt
file Desktop/shot.png
file Downloads/installer.dmg

# A symlink trap: everything reached through it is refused.
ln -sfn /System link

file Applications/Example.app/Contents/Info.plist
file Applications/Example.app/Contents/MacOS/Example

file Projects/webapp/package.json '{"name":"webapp"}'
file Projects/webapp/node_modules/left-pad/index.js
file Projects/webapp/src/main.js
