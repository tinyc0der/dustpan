"""Summarize raw S1 runs into redacted tables.

Usage: python3 summarize.py RAW_RUN_DIR OUT_DIR

Top-level entry names of ROOT other than common macOS and toolchain folders are
replaced by an HMAC label, so the committed tables show sizes without listing
what the folder contains. The key lives outside the repo (S1_LABEL_KEY_FILE).
"""

import hashlib
import hmac
import json
import os
import statistics
import sys
from pathlib import Path

KEEP = {
    "Applications", "Desktop", "Documents", "Downloads", "Library", "Movies", "Music", "Pictures",
    "Public", ".Trash", ".cache", ".cargo", ".rustup", ".npm", ".gradle", ".m2", ".docker",
    "OrbStack", ".orbstack", "go", "Projects", "Developer", ".local", ".config",
}
METHODS = ["mole", "du", "jwalk", "bulk"]

raw, out = Path(sys.argv[1]), Path(sys.argv[2])
out.mkdir(parents=True, exist_ok=True)
key = Path(os.environ.get("S1_LABEL_KEY_FILE", Path.home() / ".claude/orchestrate/dustpan-m0/s1-label.key")).read_bytes()


def label(name):
    if name in KEEP:
        return name
    return "entry-" + hmac.new(key, name.encode(), hashlib.sha256).hexdigest()[:8]


def load_native(path):
    meta, entries = {}, {}
    for line in path.read_text().splitlines():
        parts = line.split("\t")
        if parts[0] == "entry":
            entries[parts[1]] = int(parts[2])
        else:
            meta[parts[0]] = parts[1]
    return meta, entries


def load_mole(path):
    doc = json.loads(path.read_text())
    entries = {e["name"]: e["size"] for e in doc["entries"]}
    states = {e["name"]: e["scan_status"] for e in doc["entries"]}
    return {"total": doc["total_size"], "scan_status": doc["scan_status"], "files": doc.get("total_files", 0)}, entries, states


times = [line.split("\t") for line in (raw / "times.tsv").read_text().splitlines()[1:]]
runs = sorted({int(t[1]) for t in times})
wall = {m: [float(t[2]) for t in times if t[0] == m] for m in METHODS}

per_entry = {m: {} for m in METHODS}
totals = []
mole_states = {}
for i in runs:
    meta, entries, states = load_mole(raw / f"raw-mole-{i}.json")
    mole_states.update(states)
    totals.append(("mole", i, meta["total"], meta["files"], meta["scan_status"]))
    for name, size in entries.items():
        per_entry["mole"].setdefault(name, []).append(size)
    for m in METHODS[1:]:
        meta, entries = load_native(raw / f"raw-{m}-{i}.tsv")
        totals.append((m, i, int(meta["total"]), int(meta["files"]), f"errors={meta['errors']}"))
        for name, size in entries.items():
            per_entry[m].setdefault(name, []).append(size)

with open(out / "times.tsv", "w") as f:
    f.write((raw / "times.tsv").read_text())
with open(out / "machine.tsv", "w") as f:
    f.write((raw / "machine.tsv").read_text().replace(str(Path.home()), "~"))
with open(out / "totals.tsv", "w") as f:
    f.write("method\trun\ttotal_bytes\tfiles\tstatus\n")
    for row in totals:
        f.write("\t".join(map(str, row)) + "\n")

med = {m: {n: statistics.median(v) for n, v in per_entry[m].items()} for m in METHODS}
names = sorted(set().union(*[set(med[m]) for m in METHODS]), key=lambda n: -max(med[m].get(n, 0) for m in METHODS))
with open(out / "entries.tsv", "w") as f:
    f.write("entry\tmole_status\t" + "\t".join(f"{m}_median_bytes" for m in METHODS) + "\n")
    for n in names:
        cells = [str(int(med[m][n])) if n in med[m] else "absent" for m in METHODS]
        f.write(f"{label(n)}\t{mole_states.get(n, 'not_listed')}\t" + "\t".join(cells) + "\n")

mole_total = statistics.median(t[2] for t in totals if t[0] == "mole")
mole_listed = set(med["mole"])
mole_wall = statistics.median(wall["mole"])
with open(out / "summary.tsv", "w") as f:
    f.write("method\tmedian_wall_s\tmin_wall_s\tmax_wall_s\ttime_vs_mole\tmedian_total_bytes\ttotal_vs_mole_pct\tlisted_entries_total_bytes\tlisted_vs_mole_pct\n")
    for m in METHODS:
        tot = statistics.median(t[2] for t in totals if t[0] == m)
        listed = sum(v for n, v in med[m].items() if n in mole_listed)
        mole_listed_total = sum(med["mole"].values())
        f.write(
            f"{m}\t{statistics.median(wall[m]):.1f}\t{min(wall[m]):.1f}\t{max(wall[m]):.1f}\t{statistics.median(wall[m]) / mole_wall:.2f}"
            f"\t{int(tot)}\t{(tot - mole_total) / mole_total * 100:+.2f}\t{int(listed)}\t{(listed - mole_listed_total) / mole_listed_total * 100:+.2f}\n"
        )
    skipped = [n for n in med["du"] if n not in mole_listed]
    skipped_bytes = sum(med["du"][n] for n in skipped)
    f.write(f"\n# entries du sized that mole did not list\t{len(skipped)}\t{int(skipped_bytes)} bytes\t{','.join(sorted(label(n) for n in skipped if med['du'][n] > 0))}\n")
print((out / "summary.tsv").read_text())
