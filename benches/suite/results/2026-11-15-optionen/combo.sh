#!/bin/sh
# combo.sh <name> <cmd> ...: Kombinationen der Schalter auf dem zusammengefuehrten Stand messen.
#   build <label>=<features|none> ...   baut die Suite je Label, Binary nach bins/<label>
#   run <labels|ALL> <zelle>...         ein Lauf je Zelle und Label (Instruktionen sind je Binary fest) -> results/combo-<name>.tsv
#   time <labelA> <labelB> <n> <zelle>...   Zeit im Wechsel, n Runden -> results/combo-time-<name>.tsv
#   env:  SEESAW_PDE_DUMP wird durchgereicht
set -u
n=$1; cmd=$2; shift 2
# CGU=1: eine Codegen-Einheit (Vergleiche zwischen Schalterstellungen sind sonst durch die
# Aufteilung des Crates auf Einheiten um bis zu einige Prozent verfaelscht), eigene Verzeichnisse.
x=${CGU:+-cgu$CGU}
[ -n "${CGU:-}" ] && export CARGO_PROFILE_BENCH_CODEGEN_UNITS=$CGU
D=/root/seesaw-opt/$n/src; R=/root/seesaw-opt/results; B=/root/seesaw-opt/$n/bins$x; T=$R/combo-$n$x.tsv
mkdir -p $R $B; cd $D || exit 1
case $cmd in
  build)
    for spec in "$@"; do
      label=${spec%%=*}; f=${spec#*=}; ff=""; [ "$f" != none ] && ff="--features $f"
      bin=$(CARGO_TARGET_DIR=target/combo$x cargo bench -q --bench suite $ff --no-run --message-format=json 2>/tmp/$n-combo.err | grep -o '"executable":"[^"]*/suite-[^"]*"' | tail -1 | sed 's/^"executable":"//; s/"$//')
      if [ -n "$bin" ] && [ -x "$bin" ]; then
        cp "$bin" $B/$label && grep -v "^$label	" $B/LABELS > $B/LABELS.tmp 2>/dev/null; echo "$label	$f" >> $B/LABELS.tmp; mv $B/LABELS.tmp $B/LABELS; echo "gebaut $label [$f] $(basename $bin)"
      else
        echo "BUILD_FAILED $label [$f]"; grep -E '^error' -A12 /tmp/$n-combo.err | head -40
      fi
    done ;;
  run)
    labels=$1; shift
    [ "$labels" = ALL ] && labels=$(cut -f1 $B/LABELS | tr '\n' ',')
    touch $T
    for label in $(echo $labels | tr ',' ' '); do
      for c in "$@"; do
        grep -q "^$label	$c	" $T && continue
        timeout 200 $B/$label run $c --samples 1 --label $label --tsv $T >/dev/null 2>>/tmp/$n-combo-run.err || echo "RUN_FAILED $label $c"
      done
    done
    echo "Zeilen: $(wc -l < $T)" ;;
  time)
    a=$1; b=$2; rounds=$3; shift 3; TT=$R/combo-time-$n$x.tsv
    for c in "$@"; do
      i=0; while [ $i -lt $rounds ]; do i=$((i+1))
        timeout 200 $B/$a run $c --samples 2 --label $a --tsv $TT >/dev/null 2>&1 || echo "FAILED $a $c"
        timeout 200 $B/$b run $c --samples 2 --label $b --tsv $TT >/dev/null 2>&1 || echo "FAILED $b $c"
      done
    done
    awk -F'\t' -v a=$a -v b=$b '{k=$1 SUBSEP $2; if(!(k in t)||$4+0<t[k])t[k]=$4+0; if(!s[$2]++)o[n++]=$2}
      END {for(i=0;i<n;i++){c=o[i]; if(((a SUBSEP c) in t) && ((b SUBSEP c) in t)) printf "%s | %s %.1f ms | %s %.1f ms | Faktor %.3f\n", c, a, t[a,c], b, t[b,c], t[a,c]/t[b,c]}}' $TT ;;
  *) echo "unbekannt: $cmd"; exit 2 ;;
esac
