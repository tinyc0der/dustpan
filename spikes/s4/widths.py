import sys
from pathlib import Path

out = Path(sys.argv[1])
audits = {p.stem.removeprefix("audit-"): p for p in sorted(out.glob("audit-*.tsv"))}
table = {}
for term, path in audits.items():
    for line in path.read_text().splitlines()[1:]:
        glyph, cps, uw, adv, _ = line.split("\t")
        table.setdefault((glyph, cps, uw), {})[term] = adv

terms = list(audits)
print("glyph\tcodepoints\tunicode_width\t" + "\t".join(terms) + "\tall_agree")
for (glyph, cps, uw), advs in table.items():
    cells = [advs.get(t, "-") for t in terms]
    print(f"{glyph}\t{cps}\t{uw}\t" + "\t".join(cells) + f"\t{all(c == uw for c in cells)}")
