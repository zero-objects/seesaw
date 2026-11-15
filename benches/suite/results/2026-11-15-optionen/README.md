# Optionen A bis M: Kombinationen und Profile, 2026-11-15, codepit

Stand `perf/options` @ 7a3cd4c, Board-Reservierung 986. Der Rechner war
nicht ruhig (Load 18 bis 29). Instruktionszahlen hängen nicht an der
Last, Zeiten schon.

Die Messbinaries sind mit einer Codegen-Einheit gebaut
(`CARGO_PROFILE_BENCH_CODEGEN_UNITS=1`). Im Standard-Build verschiebt
ein neuer Schalter die Aufteilung des Crates auf 16 Einheiten, das
verfälscht Vergleiche zwischen Schalterstellungen um bis zu einige
Prozent.

## Schalterstellungen (Spalte 1 der TSV)

| Label | Features |
|---|---|
| `none` | keine |
| `all` | `opt_all` (a b c d e f g h i j m) |
| `only_x` | nur `opt_x` |
| `without_x` | `opt_all` ohne `opt_x` |
| `fast` | a b e f g i m |

## Dateien

- `combo-all-cgu1.tsv`: ein Lauf je Zelle und Schalterstellung, 20
  Zellen, 25 Schalterstellungen. Spalten wie in der Suite: Label,
  Zelle, Samples, User-CPU min und median (ms), Wall (ms), Peak-RSS
  (MB), Schritte, angewandte Schritte, Knoten, Fingerprint, dann
  `instr=`, `cycles=`, `llc_miss=`, `l1d_miss=`, `br_miss=`.
  Fingerprint, angewandte Schritte und Knotenzahl sind je Zelle über
  alle Schalterstellungen gleich. `cycles` und die Fehlzugriffe sind
  wegen der Last nicht verwertbar.
- `time-all.tsv`: Zeit im Wechsel, je Runde und Schalterstellung ein
  Lauf mit zwei Samples. Ausgewertet wird das Minimum der User-CPU.
- `j-all.tsv`: Zellen des parallelen Seeds mit dem Binary `all`, drei
  Samples. Spalte 6 ist die Wall-Zeit.
- `fp-<none|all>-<zelle>.txt`: `hot.sh` mit `SUITE_CALLGRAPH=fp`,
  Ereignis `instructions:u`. `fpmiss-…`: dasselbe für `cache-misses:u`.
  Standard-Build mit Frame-Pointern, nicht die Messbinaries.
- `checks.json`: Prüfungen der Kombinationen mit Ergebnis.
- `combo.sh`, `opt.sh`: die Aufrufe auf codepit. `combo_report.py`,
  `hotdiff.py`: Auswertung als Tabellen.

Die pde-Zellen brauchen `SEESAW_PDE_DUMP` (Protokolle, nicht im
Repository, siehe `../2026-11-15-pde/README.md`).

## Nicht in dieser Reihe

- Jede Option einzeln durch Gate, `seesaw_checks` und Differenztest:
  von den Strängen gefahren (`perf/opt-1` bis `perf/opt-3`), hier nicht
  wiederholt.
- s390x, `cargo publish --dry-run`, Konsumenten und pde-Tests mit den
  Schaltern.
- Eine Zeitmessung auf ruhigem Rechner.
