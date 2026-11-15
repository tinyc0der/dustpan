import statistics
import sys
from collections import defaultdict
from pathlib import Path

out = Path(sys.argv[1])
rows = []
for line in (out / "suite.tsv").read_text().splitlines():
    if line.startswith(("#", "rep\t")):
        continue
    rep, method, item, ok, ms, gone, landed, in_trash, _detail = (line.split("\t") + [""] * 9)[:9]
    rows.append((int(rep), method, item, ok == "true", float(ms), gone == "true", landed, in_trash == "true"))

print("method\titem\truns\tall_ok\tall_source_gone\tall_in_dot_trash\tmedian_ms\tlanded_example")
cells = defaultdict(list)
for r in rows:
    cells[(r[1], r[2])].append(r)
for (method, item), rs in cells.items():
    print(
        f"{method}\t{item}\t{len(rs)}\t{all(r[3] for r in rs)}\t{all(r[5] for r in rs)}\t{all(r[7] for r in rs)}"
        f"\t{statistics.median(r[4] for r in rs):.2f}\t{rs[-1][6]}"
    )

print()
print("method\tmedian_ms_all_items\tmedian_ms_excluding_rep1")
by_method = defaultdict(list)
for r in rows:
    by_method[r[1]].append(r)
for method, rs in by_method.items():
    warm = [r[4] for r in rs if r[0] > 1] or [r[4] for r in rs]
    print(f"{method}\t{statistics.median(r[4] for r in rs):.2f}\t{statistics.median(warm):.2f}")
