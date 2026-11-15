#!/usr/bin/env python3
"""Auswertung der Kombinationsmessung: combo_report.py <combo.tsv> [spalte: instr|rss|llc_miss|cycles]

Labels: none, all, only_<x>, without_<x>, sonst frei. Ausgabe als Markdown:
je Zelle der Wert von none und all, die Wirkung jeder Option allein (gegen none)
und ihr Beitrag im Gesamtstand (Wert ohne sie gegen all).
"""
import sys
from collections import OrderedDict

path = sys.argv[1]
col = sys.argv[2] if len(sys.argv) > 2 else 'instr'
val, fp, cells, labels = {}, {}, OrderedDict(), OrderedDict()
for line in open(path):
    f = line.rstrip('\n').split('\t')
    if len(f) < 12:
        continue
    label, cell = f[0], f[1]
    d = {'rss': float(f[6]), 'user': float(f[3])}
    for kv in f[11:]:
        if '=' in kv:
            k, v = kv.split('=', 1)
            try:
                d[k] = float(v)
            except ValueError:
                pass
    if col not in d:
        continue
    val[label, cell] = d[col]
    fp[label, cell] = f[10]
    cells[cell] = 1
    labels[label] = 1

opts = sorted({l.split('_', 1)[1] for l in labels if l.startswith('only_') or l.startswith('without_')})
scale = 1e6 if col in ('instr', 'cycles', 'llc_miss', 'l1d_miss', 'br_miss') else 1


def num(x):
    if x is None:
        return '–'
    x /= scale
    return f'{x:,.0f}'.replace(',', ' ') if x >= 1000 else f'{x:.1f}'.replace('.', ',')


def pct(a, b):
    if a is None or b is None or b == 0:
        return '–'
    return f'{(a - b) / b * 100:+.1f}'.replace('.', ',')


bad = [(l, c) for (l, c) in fp if ('none', c) in fp and fp[l, c] != fp['none', c]]
print(f'Spalte {col}. Fingerprints abweichend von none: {len(bad)} {bad[:6]}')
print()
print('| Zelle | none | all | all gegen none % |')
print('|---|---|---|---|')
for c in cells:
    n, a = val.get(('none', c)), val.get(('all', c))
    print(f'| `{c}` | {num(n)} | {num(a)} | {pct(a, n)} |')
print()
print('Option allein gegen none, in %:')
print('| Zelle | ' + ' | '.join(opts) + ' |')
print('|---|' + '---|' * len(opts))
for c in cells:
    n = val.get(('none', c))
    print(f'| `{c}` | ' + ' | '.join(pct(val.get((f'only_{o}', c)), n) for o in opts) + ' |')
print()
print('Beitrag im Gesamtstand: all gegen den Stand ohne die Option, in % (negativ: die Option spart dort):')
print('| Zelle | ' + ' | '.join(opts) + ' |')
print('|---|' + '---|' * len(opts))
for c in cells:
    a = val.get(('all', c))
    print(f'| `{c}` | ' + ' | '.join(pct(a, val.get((f'without_{o}', c))) for o in opts) + ' |')
extra = [l for l in labels if l not in ('none', 'all') and not l.startswith('only_') and not l.startswith('without_')]
if extra:
    print()
    print('Weitere Kombinationen gegen none, in %:')
    print('| Zelle | ' + ' | '.join(extra) + ' |')
    print('|---|' + '---|' * len(extra))
    for c in cells:
        n = val.get(('none', c))
        print(f'| `{c}` | ' + ' | '.join(pct(val.get((l, c)), n) for l in extra) + ' |')
