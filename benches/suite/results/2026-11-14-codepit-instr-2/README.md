# Messreihe 2026-11-14 (dritte), codepit, Instruktionszahl

Wie `../2026-11-14-codepit-instr/`, Board-Reservierung 911.

- `one.tsv`: ein Lauf je Zelle und Stand.
- `ab-hj-hz-*.tsv`: Stand hj gegen hz, verschränkt.
- `trace-final2.tsv`: Strukturzähler des Stands hj.

| Label | Engine-Commit | Stand |
|---|---|---|
| hz | 9f65969 | Ende der zweiten Messreihe |
| ht1 | c01be20 | übersetzte Matches auf eigenen Knoten nicht eintragen |
| hu1 | e544d6a, `src/ident.rs` aus dem Vorgänger | Typbereich der Participation-Liste |
| hu | e544d6a | dazu Hash von GhostId als ein Wort |
| hq | 8697742 | applied als Hash-Menge, created ohne Kopie |
| hk | 7d33840 | Slot einmal schreiben, Kandidaten verteilen |
| hj | a77c94f | hk ohne das einmalige Schreiben |
| hg | 8a06df9 | Gruppenliste im Arbeitsspeicher |
| he, hf | 9abb9ae | hg mit dem einmaligen Schreiben (Endstand) |
