# Messreihe 2026-11-14 (zweite), codepit, Instruktionszahl

Rechner und Aufruf wie in `../2026-11-14-codepit/`, Board-Reservierung
909, RAM-Grenze 12 GB. Die User-CPU-Zeit streute zwischen Läufen
desselben Binaries um 10 bis 15 % (load average 9 bis 14 durch andere
Container). Vergleichsgröße ist deshalb die Zahl der im User-Modus
ausgeführten Instruktionen der gemessenen Strecke (Spalte `instr=`). Sie
war zwischen Läufen desselben Stands auf 0,1 Mio. gleich.

Jeder Stand ist der Engine-Code des genannten Commits mit der Suite
(`benches/`) des jeweils aktuellen HEAD.

- `one.tsv`: ein Lauf je Zelle und Stand.
- `ab-hz-anchor-*.tsv`: Endstand gegen AnchorIndex-Stand, verschränkt,
  je Zelle zwei Durchgänge mit je zwei Läufen je Stand.
- `base-final.tsv`: 2.0.2, zwei Läufe je Zelle.
- `trace-final.tsv`: Strukturzähler des Endstands.

| Label | Engine-Commit | Stand |
|---|---|---|
| base | f2b0e53 | 2.0.2 |
| anchor | a5a264b | AnchorIndex |
| vb | 891a63b | bis Link-Prüfung an der Verbindung |
| vc | 1bc9590 | vb mit vorab gebautem Plan-Cache |
| h0 | 1482510 | Ende der ersten Messreihe |
| hy | c7ed337, `src/ident.rs` aus 24f8e7a | Gang ohne Status, Plätze nach eigenem Wert |
| hr | f27f628 | Ordnung von GhostId ausgeschrieben, Ref-Folge geteilt |
| hh | 1faffb6 | Verbindungs-ID aus der Participation-Liste (auch in connected) |
| hc | 69a96a2 | connect_reporting mit weniger Zugriffen |
| hi | bc09838 | Knoten-IDs einmal abgeleitet |
| hm | 0728429 | Kandidaten ohne Zwischenliste, Gang endet am Pfadende |
| hv | 267ec36 | Blattwert geliehen |
| hw | 5f80eb9 | connected leitet die ID wieder ab |
| hp | ec9f248 | Participation-Liste als sortierter Vektor |
| hs | 1a53437 | Slot als Enum |
| hl | 94df4a3 | Plan-Cache, Bau bei erster Suche |
| hx | 5661116 | Index-Listen bis vier Einträge ohne Allokation |
| hn | 5d270ee | verankerte Suche ohne eigene Allokationen |
| ht | f335f0f | Todo-Schlüssel mit Kopf |
| hb | 0ee58da | Identität in einem Hash-Aufruf (zurückgenommen) |
| hz | 95b2f67 | Endstand |
