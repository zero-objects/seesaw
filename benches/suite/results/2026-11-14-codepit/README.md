# Messreihe 2026-11-14, codepit

Rechner: codepit (AMD Ryzen Threadripper PRO 3955WX, LXC, rustc 1.95.0),
exklusiv reserviert (Board, Reservierung 908), RAM-Grenze 10 GB.
Aufruf je Zelle: `benches/suite/cells.sh <label> <dir> <samples> <cell>...`.

`time.tsv`: label, Zelle, Läufe, User-CPU Minimum (ms), User-CPU Median
(ms), Wall Minimum (ms), Peak-RSS (MB), Schritte, angewandte Schritte,
Knoten, Fingerprint. `trace.tsv`: dieselben Spalten aus einem Lauf mit
`--features perf_trace`, danach die Zähler.

| Label | Commit | Stand |
|---|---|---|
| base | 9588008 (langpack), f2b0e53 (f2p) | 2.0.2 mit Suite und Zählern |
| anchor | a5a264b | AnchorIndex |
| a | 0ab625a | Plätze über markierte Pfadknoten |
| b | 891a63b | erster Link an der Verbindung, Abschluss nur offene Links |
| c | 1bc9590 | Plan-Cache (zurückgenommen in d7de34e) |
| d | d7de34e | wie b, mit f2p in der Suite |
| e | 70dc8a8 | Match mit mehreren neuen Knoten nur vom ersten aus |

Die Stufe `f2p/*/f200000` ist nicht enthalten: der Lauf überschritt die
RAM-Grenze (cgroup-OOM bei anon-rss 10,4 GB).
