#!/bin/sh
# step.sh: naechster Schritt der Messleiter (Brief task-14-messung-brief.md), ein Aufruf je Board-Job.
# Reihenfolge je Runde r = 1..3: Java CELLS (jsuite.sh ladder), Java pde java40, Rust sel und par.
# Exit 0: weiter aufrufen. Exit 3: alles fertig. Exit 1: Fehler, Zeile FEHLER im Log.
# Ein Lauf, der zweimal scheitert, wird in results/step-fehler.txt notiert und uebersprungen.
set -u
R=/root/seesaw-java; B=$R/src/java/bench; RES=$R/results
export SEESAW_PDE_DUMP=$R/pde-dump
LIMIT=270; start=$(date +%s)
STATE=$RES/step.state; FAIL=$RES/step-fehler.txt
PDE_STAGES="L0 T4 T5 T6 T7 T8 T9 T10 T11 T12 T13 Ende"
PDE_CELLS="pde/load/java40 pde/graph/java40 pde/open/java40"
SEL_CELLS="langpack/add/k700/n2000 langpack/seed/k700/n2000 langpack/backward/k700/n2000
langpack/add/k700/n20000 langpack/delta-add/k700/n20000/d200
langpack/delta-change/k700/n20000/d200 langpack/delta-delete/k700/n20000/d200
langpack/delta-delete/k700/n20000/d2000 f2p/batch/f20000 f2p/add/f20000
f2p/delta-add/f20000/d1000 f2p/delta-delete/f20000/d1000
langpack/engines/k700/n200/e34 langpack/index/k700/e100
pde/load/java40 pde/graph/java40 pde/open/java40"
PAR_CELLS="langpack/reseed/k700/n20000/t1 langpack/reseed/k700/n20000/t4 langpack/reseed/k700/n20000/t8
f2p/reseed/f20000/t1 f2p/reseed/f20000/t4 f2p/reseed/f20000/t8"
mkdir -p $RES; touch $STATE $FAIL
echo "$(date -u +%FT%TZ) $(uptime)" >> $RES/step-uptime.txt

left() { echo $(( LIMIT - ($(date +%s) - start) )); }

# 0. Rust-Binaries sel (ohne Features) und par (parallel_seed) bauen, falls sie fehlen
for spec in sel:none par:parallel_seed; do
  l=${spec%%:*}; f=${spec#*:}; ff=""; [ "$f" != none ] && ff="--features $f"
  [ -x $R/bins/$l ] && continue
  mkdir -p $R/bins
  exe=$(cd $R/src && CARGO_PROFILE_BENCH_CODEGEN_UNITS=1 CARGO_TARGET_DIR=$R/target-rust timeout 250 cargo bench --offline -q --bench suite $ff --no-run --message-format=json 2>$RES/rbuild-$l.err | grep -o '"executable":"[^"]*/suite-[^"]*"' | tail -1 | sed 's/^"executable":"//; s/"$//')
  if [ -n "$exe" ] && [ -x "$exe" ]; then cp "$exe" $R/bins/$l; echo "gebaut $l"; elif grep -q "^error" $RES/rbuild-$l.err; then echo "FEHLER bau $l"; tail -5 $RES/rbuild-$l.err; exit 1
  else echo "bau $l nicht fertig, erneut"; fi
  exit 0
done
# Zeilen einer TSV mit Label $2 und Zelle $3 (Tab-getrennt, Spalte 1 und 2)
count() { if [ -f "$1" ]; then awk -F'\t' -v l="$2" -v c="$3" '$1==l && $2==c' "$1" | wc -l; else echo 0; fi; }
# geschaetzte Dauer (Sekunden) eines Laufs (Label $2, Zelle $3) aus einer Dauerdatei "<label> <zelle> <s>":
# laengste Dauer desselben Paars, sonst die zuletzt gemessene Dauer der Zelle, sonst 0
longest() { if [ -f "$1" ]; then awk -v l="$2" -v c="$3" '$2==c {last=$3; if ($1==l && $3>m) m=$3} END {print (m>0?m:last)+0}' "$1"; else echo 0; fi; }
fails() { grep -c "^$1 r=$2 $3 $4\$" $FAIL; }
done_mark() { grep -q "^$1\$" $STATE; }
# passt ein Lauf mit bekannter Dauer d noch in die Zeit? Ohne bekannte Dauer nur als erster Lauf.
fits() { d=$1; first=$2; if [ "$d" -eq 0 ]; then [ "$first" = 1 ]; else [ $(( d * 5 / 4 )) -lt "$(left)" ]; fi; }

for r in 1 2 3; do
  # 1. Java CELLS, Runde r
  if ! done_mark "r$r cells"; then
    out=$(cd $B && ROUNDS=$r LIMIT=$(left) sh jsuite.sh ladder --tsv ladder.tsv 2>&1 | tail -3)
    echo "$out"
    if echo "$out" | grep -q "ladder fertig"; then echo "r$r cells" >> $STATE; fi
    echo "schritt r=$r cells"; exit 0
  fi
  # 2. Java pde java40, Runde r, ein Prozess je Stand und Zelle
  if ! done_mark "r$r pde"; then
    ran=0; open=0
    for c in $PDE_CELLS; do for l in $PDE_STAGES; do
      [ "$(count $RES/ladder-pde.tsv $l $c)" -ge $r ] && continue
      [ "$(fails pde $r $l $c)" -ge 2 ] && continue
      first=$([ $ran = 0 ] && echo 1 || echo 0)
      if fits "$(longest $RES/ladder-pde.tsv.dauer $l $c)" $first; then
        before=$(count $RES/ladder-pde.tsv $l $c)
        (cd $B && LIMIT=260 sh jsuite.sh run --warmup 1 --reps 2 ladder-pde.tsv $l $c 2>&1 | tail -2)
        [ "$(count $RES/ladder-pde.tsv $l $c)" -gt "$before" ] || echo "pde r=$r $l $c" >> $FAIL
        ran=$((ran+1))
      else open=$((open+1)); fi
    done; done
    [ $open = 0 ] && [ $ran = 0 ] && echo "r$r pde" >> $STATE
    echo "schritt r=$r pde gelaufen=$ran offen=$open"; exit 0
  fi
  # 3. Rust, Runde r, ein Prozess je Zelle mit 5 Samples
  if ! done_mark "r$r rust"; then
    ran=0; open=0
    for spec in $(for c in $SEL_CELLS; do echo "sel:$c"; done; for c in $PAR_CELLS; do echo "par:$c"; done); do
      l=${spec%%:*}; c=${spec#*:}
      [ "$(count $RES/rust-ladder.tsv $l $c)" -ge $r ] && continue
      [ "$(fails rust $r $l $c)" -ge 2 ] && continue
      first=$([ $ran = 0 ] && echo 1 || echo 0)
      if fits "$(longest $RES/rust-ladder.dauer $l $c)" $first; then
        t0=$(date +%s); before=$(count $RES/rust-ladder.tsv $l $c)
        timeout 250 $R/bins/$l run $c --samples 5 --label $l --tsv $RES/rust-ladder.tsv >/dev/null 2>>$RES/rust-ladder.err
        if [ "$(count $RES/rust-ladder.tsv $l $c)" -gt "$before" ]; then echo "$l $c $(( $(date +%s) - t0 ))" >> $RES/rust-ladder.dauer
        else echo "rust r=$r $l $c" >> $FAIL; fi
        ran=$((ran+1))
      else open=$((open+1)); fi
    done
    [ $open = 0 ] && [ $ran = 0 ] && echo "r$r rust" >> $STATE
    echo "schritt r=$r rust gelaufen=$ran offen=$open"; exit 0
  fi
done
echo "alles fertig"; exit 3
