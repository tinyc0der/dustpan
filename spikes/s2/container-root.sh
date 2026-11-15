#!/usr/bin/env bash
# Usage: container-root.sh OUTFILE [METHOD...]
# Tries to trash the spike's own container folder ~/Library/Containers/com.dustpan.spike.s2
# after containermanagerd has adopted it. The default methods skip finder,
# because Finder answers a refusal with a modal alert that waits for a click.
set -u

out=${1:?outfile}
shift
methods=("${@:-rename nsfm cli}")
read -r -a methods <<< "${methods[*]}"
here=$(cd "$(dirname "$0")" && pwd)
bin=${S2TRASH:-$here/target/release/s2trash}
target="$HOME/Library/Containers/com.dustpan.spike.s2"
{
  echo "date	$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  xattr "$target" 2>&1 | sed 's/^/xattr	/'
  for m in "${methods[@]}"; do
    [[ -e $target ]] || break
    echo "$m	$(perl -e 'alarm 60; exec @ARGV' "$bin" "$m" "$target")"
  done
  [[ -e $target ]] && echo "after	still present" || echo "after	gone"
} | sed "s#$HOME#~#g" > "$out"
cat "$out"
