#!/usr/bin/env python3
"""jreport.py <tsv>: Auswertung einer TSV von JavaSuite (java/bench/jsuite.sh).

Je Zelle und Label über alle Zeilen (Runden): Thread-CPU min (Spalte 4), Wall min
(Spalte 6), alloc (Minimum, MB) und gc_ms (Minimum). Dann die Prüfung, dass Schritte,
angewandte Schritte, Knoten und Fingerprint (Spalten 8 bis 11) je Zelle über alle Labels
und Zeilen gleich sind. Exit-Code 1 bei einer Abweichung.
"""

import sys
from collections import OrderedDict


def parse(path):
    rows = []
    with open(path, encoding="utf-8") as f:
        for n, line in enumerate(f, 1):
            line = line.rstrip("\n")
            if not line:
                continue
            cols = line.split("\t")
            if len(cols) < 11:
                sys.exit(f"{path}:{n}: {len(cols)} Spalten, erwartet mindestens 11")
            extra = dict(c.split("=", 1) for c in cols[11:] if "=" in c)
            rows.append({
                "label": cols[0],
                "cell": cols[1],
                "cpu_min": float(cols[3]),
                "wall_min": float(cols[5]),
                "result": tuple(cols[7:11]),
                "alloc": int(extra["alloc"]) if "alloc" in extra else None,
                "gc_ms": int(extra["gc_ms"]) if "gc_ms" in extra else None,
            })
    return rows


def minimum(values):
    values = [v for v in values if v is not None]
    return min(values) if values else None


def main():
    if len(sys.argv) != 2:
        sys.exit("usage: jreport.py <tsv>")
    rows = parse(sys.argv[1])
    groups = OrderedDict()
    for r in rows:
        groups.setdefault(r["cell"], OrderedDict()).setdefault(r["label"], []).append(r)

    header = ("Zelle", "Label", "Zeilen", "CPU min ms", "Wall min ms", "alloc MB", "gc_ms")
    table = []
    for cell, labels in groups.items():
        for label, rs in labels.items():
            alloc = minimum(r["alloc"] for r in rs)
            gc = minimum(r["gc_ms"] for r in rs)
            table.append((
                cell,
                label,
                str(len(rs)),
                f"{min(r['cpu_min'] for r in rs):.1f}",
                f"{min(r['wall_min'] for r in rs):.1f}",
                "-" if alloc is None else f"{alloc / 2**20:.1f}",
                "-" if gc is None else str(gc),
            ))
    widths = [max(len(row[i]) for row in [header] + table) for i in range(len(header))]
    for row in [header] + table:
        print("  ".join(v.ljust(w) if i < 2 else v.rjust(w)
                        for i, (v, w) in enumerate(zip(row, widths))).rstrip())

    print()
    print("Gleichheit je Zelle (Schritte, angewandte Schritte, Knoten, Fingerprint):")
    bad = 0
    for cell, labels in groups.items():
        results = OrderedDict()
        for label, rs in labels.items():
            for r in rs:
                results.setdefault(r["result"], []).append(label)
        if len(results) == 1:
            print(f"{cell} gleich {' '.join(next(iter(results)))} labels={','.join(labels)}")
        else:
            bad += 1
            parts = "; ".join(f"{' '.join(res)} bei {','.join(OrderedDict.fromkeys(ls))}"
                              for res, ls in results.items())
            print(f"{cell} ABWEICHUNG {parts}")
    print(f"zellen={len(groups)} abweichend={bad}")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
