#!/usr/bin/env python3
"""hotdiff.py <hot-vorher.txt> <total-vorher> <hot-nachher.txt> <total-nachher> [zeilen]

Vergleicht zwei Profile von hot.sh in absoluten Groessen: Anteil je Funktion mal
Gesamtzahl (z. B. Mio. Instruktionen der Messstrecke). Ausgabe: zwei Markdown-Tabellen
(inklusiv, Eigenanteil), sortiert nach dem Betrag der Aenderung.
"""
import re
import sys


def read(path):
    sec, d = None, {'incl': {}, 'self': {}}
    for line in open(path):
        line = line.rstrip('\n')
        if line.startswith('-- inclusive'):
            sec = 'incl'
        elif line.startswith('-- self'):
            sec = 'self'
        elif sec and '\t' in line:
            pct, sym = line.strip().split('\t', 1)
            d[sec][name(sym)] = d[sec].get(name(sym), 0.0) + float(pct)
    return d


def name(s):
    if s.startswith('_ZN'):
        i, parts = 3, []
        while i < len(s) and s[i].isdigit():
            j = i
            while s[j].isdigit():
                j += 1
            n = int(s[i:j])
            parts.append(s[j:j + n])
            i = j + n
        if parts and re.fullmatch(r'h[0-9a-f]{16}', parts[-1]):
            parts.pop()
        s = '::'.join(parts).replace('$LT$', '<').replace('$GT$', '>').replace('..', '::')
    s = s.replace('_$u7b$$u7b$closure$u7d$$u7d$', '{{closure}}').replace('$u7b$', '{').replace('$u7d$', '}')
    s = s.replace('seesaw_tgg::', '')
    s = re.sub(r'<core::ops::try_trait::Wrapped<T,A,F> as core::ops::function::FnMut<\(A,\)>>::call_mut', 'core Wrapped::call_mut', s)
    s = re.sub(r'<core::iter::adapters::map::Map<I,F> as core::iter::traits::iterator::Iterator>::try_fold', 'core Map::try_fold', s)
    return s[:80]


a, ta, b, tb = read(sys.argv[1]), float(sys.argv[2]), read(sys.argv[3]), float(sys.argv[4])
rows = int(sys.argv[5]) if len(sys.argv) > 5 else 22
for kind, title in (('incl', 'inklusiv'), ('self', 'Eigenanteil')):
    keys = set(a[kind]) | set(b[kind])
    tab = []
    for k in keys:
        va = a[kind].get(k, 0.0) * ta / 100
        vb = b[kind].get(k, 0.0) * tb / 100
        tab.append((abs(vb - va), k, va, vb))
    tab.sort(reverse=True)
    print(f'{title}: vorher gesamt {ta:.0f}, nachher gesamt {tb:.0f}')
    print('| Funktion | vorher | nachher | Änderung |')
    print('|---|---|---|---|')
    for _, k, va, vb in tab[:rows]:
        print(f'| `{k}` | {va:.0f} | {vb:.0f} | {vb - va:+.0f} |')
    print()
