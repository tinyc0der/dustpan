"""Parse candidate lists from mo dry runs against the S5 fixture.

Usage: python3 parse.py OUTDIR
"""

import re
import sys
from pathlib import Path

out = Path(sys.argv[1])
ANSI = re.compile(r"\x1b\[[0-9;?]*[A-Za-z]|\x1b\][^\x07]*\x07|\r")


def text(name):
    p = out / name
    return ANSI.sub("", p.read_text(errors="replace")) if p.exists() else ""


rows = []
for line in text("mole-state/.config_mole_clean-list.txt").splitlines():
    line = line.strip()
    if not line or line.startswith(("#", "=")):
        continue
    path, _, note = line.partition("  # ")
    rows.append(("clean", "clean-list.txt", path.strip(), note.strip()))

for line in text("purge.stdout").splitlines():
    m = re.search(r"\[DRY RUN\]\s+(.*?),\s*([^,]+)$", line)
    if m:
        rows.append(("purge", "stdout [DRY RUN]", m.group(1).strip(), m.group(2).strip()))

for line in text("installer.stdout").splitlines():
    m = re.search(r"([\w.@+-][^|]*?\.(?:dmg|pkg|mpkg|iso|xip|zip))\s+(\S+\s*[KMGT]?B)\s*\|\s*(\S+)", line)
    if m:
        rows.append(("installer", "stdout menu row", m.group(1).strip(), f"{m.group(2)} | {m.group(3)}"))


def in_fixture(cmd, path):
    if cmd == "installer":
        return "basename only"
    return str(path.startswith(("$FIXTURE/", "~/")))


print("command\tsource\tcandidate\tdetail\tin_fixture")
for r in rows:
    print("\t".join(r) + "\t" + in_fixture(r[0], r[2]))

expect = [
    ("clean", "Library/Caches/com.example.notes", True),
    ("clean", "Library/Caches/Google/Chrome", True),
    ("clean", "Library/Caches/Firefox", True),
    ("clean", "Library/Logs/ExampleApp", True),
    ("clean", ".Trash", True),
    ("clean", "Bookmarks", False),
    ("clean", "Documents/notes.txt", False),
    ("clean", "com.trap.victim/", False),
    ("clean", "com.trap.realhome/", False),
    ("clean", "victim/canary.txt", False),
    ("purge", "Projects/oldweb/node_modules", True),
    ("purge", "Projects/oldrust/target", True),
    ("purge", "Projects/freshweb/node_modules", False),
    ("purge", "Projects/trapweb/node_modules", False),
    ("installer", "Example-1.0.dmg", True),
    ("installer", "Setup.pkg", True),
    ("installer", "ExampleInstaller.zip", True),
    ("installer", "photos.zip", False),
    ("installer", "trap.dmg", False),
]
print()
print("expectation\tcommand\tneedle\twant_listed\tlisted\tmatch")
for cmd, needle, want in expect:
    listed = any(r[0] == cmd and needle in r[2] for r in rows)
    print(f"expect\t{cmd}\t{needle}\t{want}\t{listed}\t{listed == want}")
