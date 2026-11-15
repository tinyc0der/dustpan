#!/usr/bin/env bash
# Usage: mkfixture.sh FIXTURE_ROOT
# Builds FIXTURE_ROOT/home (a fake HOME) and FIXTURE_ROOT/victim (symlink-trap targets).
set -euo pipefail

root=${1:?fixture root}
h="$root/home"
v="$root/victim"
mkdir -p "$h" "$v"

old=$(date -v-30d +%Y%m%d%H%M)
blob() { mkdir -p "$(dirname "$1")"; head -c "$2" /dev/zero > "$1"; }
age_tree() { find "$1" -exec touch -h -t "$old" {} +; }

blob "$v/canary.txt" 4096
blob "$v/node_modules/keep.js" 4096

blob "$h/Library/Caches/com.example.notes/Cache.db" 2097152
blob "$h/Library/Caches/com.example.notes/fsCachedData/blob1" 1048576
blob "$h/Library/Caches/Google/Chrome/Default/Cache/Cache_Data/data_0" 1048576
blob "$h/Library/Caches/Firefox/Profiles/abcd1234.default-release/cache2/entries/E1" 1048576
blob "$h/Library/Application Support/Google/Chrome/Default/Code Cache/js/index" 524288
blob "$h/Library/Application Support/Google/Chrome/Default/GPUCache/data_1" 524288
blob "$h/Library/Application Support/Google/Chrome/Default/Bookmarks" 2048
blob "$h/Library/Logs/ExampleApp/app.log" 1048576
blob "$h/Library/Logs/DiagnosticReports/ExampleApp-2026-10-01.ips" 65536
age_tree "$h/Library/Logs"
blob "$h/.Trash/old-report.pdf" 131072
blob "$h/Documents/notes.txt" 1024

blob "$h/Projects/oldweb/package.json" 64
blob "$h/Projects/oldweb/package-lock.json" 64
blob "$h/Projects/oldweb/node_modules/left-pad/index.js" 1048576
blob "$h/Projects/oldweb/node_modules/.package-lock.json" 64
age_tree "$h/Projects/oldweb"
blob "$h/Projects/freshweb/package.json" 64
blob "$h/Projects/freshweb/package-lock.json" 64
blob "$h/Projects/freshweb/node_modules/right-pad/index.js" 1048576
blob "$h/Projects/freshweb/node_modules/.package-lock.json" 64
blob "$h/Projects/oldrust/Cargo.toml" 64
blob "$h/Projects/oldrust/target/debug/oldrust" 2097152
blob "$h/Projects/oldrust/target/.rustc_info.json" 64
age_tree "$h/Projects/oldrust"
mkdir -p "$h/Projects/trapweb"
blob "$h/Projects/trapweb/package.json" 64
ln -s "$v/node_modules" "$h/Projects/trapweb/node_modules"
age_tree "$h/Projects/trapweb"

ln -s "$v" "$h/Library/Caches/com.trap.victim"
ln -s "$HOME/Documents" "$h/Library/Caches/com.trap.realhome"
ln -s "$v" "$h/Library/Logs/trap-logs"

mkdir -p "$h/Downloads"
blob "$h/Downloads/Example-1.0.dmg" 1048576
blob "$h/Downloads/Setup.pkg" 524288
stage=$(mktemp -d)
mkdir -p "$stage/Example.app/Contents"
printf '<plist/>\n' > "$stage/Example.app/Contents/Info.plist"
(cd "$stage" && zip -qr "$h/Downloads/ExampleInstaller.zip" Example.app)
printf 'hello\n' > "$stage/photo.txt"
(cd "$stage" && zip -q "$h/Downloads/photos.zip" photo.txt)
rm -rf "$stage"
ln -s "$v/canary.txt" "$h/Downloads/trap.dmg"
