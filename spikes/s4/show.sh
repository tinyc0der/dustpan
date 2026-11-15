#!/bin/bash
# Usage: show.sh S4FRAME OUTDIR TAG
# Runs inside the terminal under test and resizes it to 120x40.
bin=$1
out=$2
tag=$3
printf '\033]0;dustpan-s4-%s\007' "$tag"
printf '\033[8;40;120t'
sleep 1
stty size > "$out/size-$tag.txt"
"$bin" --audit "$out/audit-$tag.tsv"
printf '\033]0;dustpan-s4-%s\007' "$tag"
"$bin" --hold 15 --marker "$out/.ready-$tag-unicode"
"$bin" --ascii --hold 15 --marker "$out/.ready-$tag-ascii"
touch "$out/.done-$tag"
