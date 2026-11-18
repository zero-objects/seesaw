#!/usr/bin/env python3
"""jreport.py <tsv>: Auswertung einer TSV von JavaSuite (java/bench/jsuite.sh).

Je Zelle und Label über alle Zeilen (Runden): Thread-CPU min (Spalte 4), Wall min
(Spalte 6), alloc (Minimum, MB), gc_ms (Minimum) und proc_cpu_ms (CPU des ganzen Prozesses,
Minimum, "-" für Zeilen ohne die Spalte oder mit -1). Dann die Prüfung, dass Schritte,
angewandte Schritte, Knoten und Fingerprint (Spalten 8 bis 11) je Zelle über alle Labels
und Zeilen gleich sind. Exit-Code 1 bei einer Abweichung.

Zeilen "<label> <zelle> nicht_verfuegbar <grund>" (eine Zelle, die der Stand nicht kann)
gehen in keine der beiden Auswertungen ein und stehen am Ende je Zelle und Label.

jreport.py --ladder <ladder.txt> --ref <tsv>[:<label>] [--ref ...] [--rust <tsv>]
           [--komma] [--kurz-md] [--alle-staende] [--out <datei>] <tsv>: Auswertung der Messleiter
(java/bench/jsuite.sh ladder). ladder.txt gibt die Reihenfolge der Stände. Je Zelle eine
Tabelle über die Stände: Thread-CPU min (Spalte 4), Wall min (Spalte 6), alloc (MB),
gc_ms und proc_cpu_ms, je das Minimum über die Runden, dazu die Zahl der Zeilen (Runden)
mit einem Wert. Danach je Zelle der Gewinn L0 gegen den letzten Stand (L0-Wert geteilt
durch den Wert am Ende) und der Faktor Java:Rust bei L0 und am Ende. Rust: Minimum über
die Zeilen der Zelle in --rust, Spalte 4 (User-CPU), bei reseed-Zellen Spalte 6 (Wall).
In den reseed-Zellen ist Wall die maßgebliche Spalte auf beiden Seiten (Ruling R25), die
Thread-CPU des aufrufenden Threads ist dort nicht vergleichbar. Fingerprint-Prüfung: die
Spalten 8 bis 11 (Schritte, angewandte Schritte, Knoten, Fingerprint) jeder Java- und
jeder Rust-Zeile gegen die Referenz derselben Zelle aus den --ref-Dateien (mit
":<label>" nur Zeilen dieses Labels). Exit-Code 1 bei einer Abweichung oder einer Zelle
ohne Referenz. --rust wertet nur die Zellen aus, die die Java-TSV hat. --komma schreibt
Dezimalkommas, --kurz-md gibt nur die Kurzform als Markdown-Tabelle aus, --out schreibt
zusätzlich in eine Datei. --alle-staende hängt an die Kurzform eine zweite Tabelle mit dem
Wert jedes Stands der Leiter (maßgebliche Spalte wie in der Kurzform) und dem Rust-Wert,
ohne Gewinn und Faktor. Ohne die Option bleibt die Ausgabe unverändert.
"""

import argparse
import sys
from collections import OrderedDict


UNAVAILABLE = "nicht_verfuegbar"


def parse(path):
    rows = []
    unavailable = OrderedDict()
    parse.unavailable_lines = 0
    with open(path, encoding="utf-8") as f:
        for n, line in enumerate(f, 1):
            line = line.rstrip("\n")
            if not line:
                continue
            cols = line.split("\t")
            if len(cols) >= 3 and cols[2] == UNAVAILABLE:
                why = cols[3] if len(cols) > 3 else ""
                parse.unavailable_lines += 1
                unavailable.setdefault((cols[1], cols[0]), why)
                continue
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
                "proc_cpu_ms": proc_cpu(extra.get("proc_cpu_ms")),
            })
    return rows, unavailable


def proc_cpu(value):
    """proc_cpu_ms einer Zeile, None ohne die Spalte oder mit -1."""
    if value is None:
        return None
    v = float(value)
    return v if v >= 0 else None


def minimum(values):
    values = [v for v in values if v is not None]
    return min(values) if values else None


def main():
    if len(sys.argv) != 2 or sys.argv[1].startswith("--"):
        ladder_main()
        return
    rows, unavailable = parse(sys.argv[1])
    groups = OrderedDict()
    for r in rows:
        groups.setdefault(r["cell"], OrderedDict()).setdefault(r["label"], []).append(r)

    header = ("Zelle", "Label", "Zeilen", "CPU min ms", "Wall min ms", "alloc MB", "gc_ms",
              "proc_cpu min ms")
    table = []
    for cell, labels in groups.items():
        for label, rs in labels.items():
            alloc = minimum(r["alloc"] for r in rs)
            gc = minimum(r["gc_ms"] for r in rs)
            proc = minimum(r["proc_cpu_ms"] for r in rs)
            table.append((
                cell,
                label,
                str(len(rs)),
                f"{min(r['cpu_min'] for r in rs):.1f}",
                f"{min(r['wall_min'] for r in rs):.1f}",
                "-" if alloc is None else f"{alloc / 2**20:.1f}",
                "-" if gc is None else str(gc),
                "-" if proc is None else f"{proc:.1f}",
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
    if unavailable:
        print()
        print("Nicht verfügbar (Zelle, Label, Grund):")
        for (cell, label), why in unavailable.items():
            print(f"{cell} {label} {why}")
    sys.exit(1 if bad else 0)


# ---------------------------------------------------------------------------
# Messleiter

DECIMAL_COMMA = False


def num(v, digits=1):
    """Zahl mit fester Nachkommastellenzahl, "-" für None."""
    if v is None:
        return "-"
    t = f"{v:.{digits}f}"
    return t.replace(".", ",") if DECIMAL_COMMA else t


def is_reseed(cell):
    return "/reseed/" in cell


def read_ladder(path):
    """Labels in der Reihenfolge von ladder.txt (Zeilen "<label> <commit>")."""
    labels = []
    with open(path, encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if line and not line.startswith("#"):
                labels.append(line.split()[0])
    return labels


def read_refs(specs):
    """Referenz je Zelle aus --ref "<tsv>[:<label>]". Zwei verschiedene Werte je Zelle sind ein Fehler."""
    refs = {}
    for spec in specs:
        path, _, label = spec.partition(":")
        rows, _ = parse(path)
        for r in rows:
            if label and r["label"] != label:
                continue
            old = refs.setdefault(r["cell"], r["result"])
            if old != r["result"]:
                sys.exit(f"{path}: Referenz der Zelle {r['cell']} uneinheitlich: "
                         f"{' '.join(old)} und {' '.join(r['result'])}")
    return refs


def check_fingerprints(name, rows, refs, out):
    """Jede Zeile gegen die Referenz ihrer Zelle. Rückgabe: (gleich, abweichend, ohne Referenz)."""
    same = bad = missing = 0
    for r in rows:
        ref = refs.get(r["cell"])
        if ref is None:
            missing += 1
            out(f"{name} {r['label']} {r['cell']} KEINE REFERENZ")
        elif ref == r["result"]:
            same += 1
        else:
            bad += 1
            out(f"{name} {r['label']} {r['cell']} ABWEICHUNG {' '.join(r['result'])} "
                f"gegen Referenz {' '.join(ref)}")
    return same, bad, missing


def table(header, rows, out, left=1):
    widths = [max(len(row[i]) for row in [header] + rows) for i in range(len(header))]
    for row in [header] + rows:
        out("  ".join(v.ljust(w) if i < left else v.rjust(w)
                      for i, (v, w) in enumerate(zip(row, widths))).rstrip())


def ratio(a, b):
    return None if a is None or b is None or b == 0 else a / b


def ladder_main():
    ap = argparse.ArgumentParser(prog="jreport.py", add_help=True)
    ap.add_argument("--ladder", required=True)
    ap.add_argument("--ref", action="append", default=[], required=True)
    ap.add_argument("--rust")
    ap.add_argument("--komma", action="store_true")
    ap.add_argument("--kurz-md", action="store_true")
    ap.add_argument("--alle-staende", action="store_true")
    ap.add_argument("--out")
    ap.add_argument("tsv")
    args = ap.parse_args()
    global DECIMAL_COMMA
    DECIMAL_COMMA = args.komma

    lines = []

    def out(text=""):
        lines.append(text)
        if not args.kurz_md:
            print(text)

    order = read_ladder(args.ladder)
    rows, unavailable = parse(args.tsv)
    unavailable_lines = parse.unavailable_lines
    refs = read_refs(args.ref)
    cells = list(OrderedDict.fromkeys(r["cell"] for r in rows))
    rust_rows = []
    if args.rust:
        rust_rows, _ = parse(args.rust)
        rust_rows = [r for r in rust_rows if r["cell"] in cells]

    by = OrderedDict()
    for r in rows:
        by.setdefault(r["cell"], {}).setdefault(r["label"], []).append(r)
    unknown = sorted({r["label"] for r in rows} - set(order))
    if unknown:
        sys.exit(f"Labels nicht in {args.ladder}: {' '.join(unknown)}")
    rust_by = OrderedDict()
    for r in rust_rows:
        rust_by.setdefault(r["cell"], []).append(r)

    out(f"Messleiter {args.tsv}: {len(rows)} Java-Zeilen, {unavailable_lines} Zeilen nicht_verfuegbar "
        f"({len(unavailable)} Paare aus Zelle und Stand), "
        f"{len(rust_rows)} Rust-Zeilen, {len(cells)} Zellen, Stände {' '.join(order)}")
    out("Werte je Stand: Minimum über die Zeilen (Runden) der Zelle. n = Zahl der Zeilen "
        "mit Wert (n_proc für proc_cpu).")
    out()

    summary = []
    per_stand = []
    header = ("Stand", "n", "CPU min ms", "Wall min ms", "alloc MB", "gc_ms", "n_proc",
              "proc_cpu min ms")
    for cell in cells:
        labels = by[cell]
        reseed = is_reseed(cell)
        out(f"== {cell}")
        if reseed:
            out("Wall min ist maßgeblich (R25). Thread-CPU misst nur den aufrufenden Thread "
                "und ist hier nicht vergleichbar.")
        table_rows = []
        stat = {}
        for label in order:
            rs = labels.get(label)
            if not rs:
                if (cell, label) in unavailable:
                    table_rows.append((label, "0", "nicht_verfuegbar", "", "", "", "", ""))
                continue
            cpu = min(r["cpu_min"] for r in rs)
            wall = min(r["wall_min"] for r in rs)
            alloc = minimum(r["alloc"] for r in rs)
            gc = minimum(r["gc_ms"] for r in rs)
            proc = minimum(r["proc_cpu_ms"] for r in rs)
            nproc = sum(1 for r in rs if r["proc_cpu_ms"] is not None)
            stat[label] = (cpu, wall)
            table_rows.append((
                label, str(len(rs)),
                num(cpu) + ("*" if reseed else ""), num(wall),
                num(None if alloc is None else alloc / 2**20),
                "-" if gc is None else str(gc), str(nproc), num(proc)))
        table(header, table_rows, out)
        if reseed:
            out("* nicht vergleichbar")
        first, last = order[0], order[-1]
        col = 1 if reseed else 0
        unit = "Wall" if reseed else "CPU"
        rrs = rust_by.get(cell, [])
        rust_v = minimum(r["wall_min"] if reseed else r["cpu_min"] for r in rrs) if rrs else None
        v0 = stat[first][col] if first in stat else None
        v1 = stat[last][col] if last in stat else None
        gain = ratio(v0, v1)
        f0 = ratio(v0, rust_v)
        f1 = ratio(v1, rust_v)
        out(f"Gewinn {first}/{last} ({unit}): {num(gain, 2)}"
            f"  Rust {unit} min: {num(rust_v)} ({len(rrs)} Zeilen)"
            f"  Faktor Java:Rust bei {first}: {num(f0)}, bei {last}: {num(f1)}")
        out()
        summary.append((cell, unit, v0, v1, gain, rust_v, f0, f1))
        per_stand.append((cell, unit, {k: v[col] for k, v in stat.items()}, rust_v))

    out(f"Kurzform ({order[0]} gegen {order[-1]}, ms, Faktor Java:Rust = Java-Wert geteilt "
        f"durch Rust-Minimum)")
    short = [(c, u, num(v0), num(v1), num(g, 2), num(rv), num(f0), num(f1))
             for c, u, v0, v1, g, rv, f0, f1 in summary]
    table(("Zelle", "Spalte", order[0], order[-1], "Gewinn", "Rust", f"Faktor {order[0]}",
           f"Faktor {order[-1]}"), short, out, left=2)
    out()
    if args.kurz_md:
        print(f"| Zelle | Spalte | {order[0]} ms | {order[-1]} ms | Gewinn | Rust ms | "
              f"Faktor {order[0]} | Faktor {order[-1]} |")
        print("|---|---|---|---|---|---|---|---|")
        for c, u, a, b, g, rv, f0, f1 in short:
            print(f"| `{c}` | {u} | {a} | {b} | {g} | {rv} | {f0} | {f1} |")

    if args.alle_staende:
        out("Alle Stände (ms, Minimum über die Zeilen, Spalte wie in der Kurzform)")
        wide = [(c, u) + tuple(num(st.get(lb)) for lb in order) + (num(rv),)
                for c, u, st, rv in per_stand]
        table(("Zelle", "Spalte") + tuple(order) + ("Rust",), wide, out, left=2)
        out()
        if args.kurz_md:
            print("| Zelle | Spalte | " + " | ".join(order) + " | Rust |")
            print("|---" * (len(order) + 3) + "|")
            for row in wide:
                print(f"| `{row[0]}` | " + " | ".join(row[1:]) + " |")

    if unavailable:
        out("Nicht verfügbar (Zelle, Label, Grund):")
        for (cell, label), why in unavailable.items():
            out(f"{cell} {label} {why}")
        out()

    out("Fingerprint (Schritte, angewandte Schritte, Knoten, Fingerprint) gegen die Referenz "
        "derselben Zelle:")
    js, jb, jm = check_fingerprints("java", rows, refs, out)
    out(f"java zeilen={len(rows)} gleich={js} abweichend={jb} ohne_referenz={jm}")
    rs_, rb, rm = check_fingerprints("rust", rust_rows, refs, out)
    out(f"rust zeilen={len(rust_rows)} gleich={rs_} abweichend={rb} ohne_referenz={rm}")
    if args.out:
        with open(args.out, "w", encoding="utf-8") as f:
            f.write("\n".join(lines) + "\n")
    sys.exit(1 if jb or jm or rb or rm else 0)


if __name__ == "__main__":
    main()
