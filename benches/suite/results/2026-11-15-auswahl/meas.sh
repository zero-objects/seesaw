#!/bin/sh
# meas.sh <cmd> ...: Messung der Auswahl auf codepit, jeder Aufruf unter 5 Minuten.
#   build <label>=<stand>:<features|none> ...   Suite mit einer Codegen-Einheit bauen, Binary nach bins/<label>
#   run <tsv> <labels,..> <zelle>...            ein Lauf je Zelle und Label -> results/<tsv>
#   time <tsv> <labels,..> <zelle>...           eine Runde Zeit im Wechsel (je Label ein Lauf mit zwei Samples)
set -u
R=/root/seesaw-sel/results; B=/root/seesaw-sel/bins; mkdir -p $R $B
export CARGO_PROFILE_BENCH_CODEGEN_UNITS=1
cmd=$1; shift
case $cmd in
  build)
    for spec in "$@"; do
      label=${spec%%=*}; rest=${spec#*=}; stand=${rest%%:*}; f=${rest#*:}; ff=""; [ "$f" != none ] && ff="--features $f"
      cd /root/seesaw-sel/$stand/src || exit 1
      bin=$(CARGO_TARGET_DIR=target/meas cargo bench -q --bench suite $ff --no-run --message-format=json 2>/tmp/meas-$label.err | grep -o '"executable":"[^"]*/suite-[^"]*"' | tail -1 | sed 's/^"executable":"//; s/"$//')
      if [ -n "$bin" ] && [ -x "$bin" ]; then cp "$bin" $B/$label && echo "gebaut $label [$stand $f] $(basename $bin)"; else echo "BUILD_FAILED $label [$stand $f]"; grep -E '^error' -A12 /tmp/meas-$label.err | head -40; fi
    done ;;
  run)
    tsv=$R/$1; labels=$2; shift 2; touch $tsv
    for label in $(echo $labels | tr ',' ' '); do
      for c in "$@"; do
        grep -q "^$label	$c	" $tsv && continue
        timeout 200 $B/$label run $c --samples 1 --label $label --tsv $tsv >/dev/null 2>>/tmp/meas-run.err || echo "RUN_FAILED $label $c"
      done
    done
    echo "Zeilen: $(wc -l < $tsv)" ;;
  time)
    tsv=$R/$1; labels=$2; shift 2
    for c in "$@"; do
      for label in $(echo $labels | tr ',' ' '); do
        timeout 200 $B/$label run $c --samples 2 --label $label --tsv $tsv >/dev/null 2>&1 || echo "FAILED $label $c"
      done
    done
    uptime ;;
  *) echo "unbekannt: $cmd"; exit 2 ;;
esac
