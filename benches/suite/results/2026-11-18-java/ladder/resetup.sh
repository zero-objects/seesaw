#!/bin/sh
# resetup.sh (Mac): /root/seesaw-java auf codepit nach einer neuen Reservierung fuer Task 4 wieder aufbauen.
# Quellen, pde-Spuren, Staende, Treiber. JARs, Messstrecke und Rust-Binaries baut step.sh selbst.
# Bereits vorhandene Ergebnisse in results/ bleiben (Dateien t4-*), nichts wird dort geloescht.
set -eu
# WORKTREE: Checkout des Repositorys, PDE_DUMP: Verzeichnis mit den pde-Spuren (*.trace).
W=${WORKTREE:?WORKTREE setzen}
D=$W/.superpowers/sdd/plan-2026-11-18
cd $W
ssh codepit 'mkdir -p /root/seesaw-java/src /root/seesaw-java/pde-dump /root/seesaw-java/results'
rsync -a --delete --exclude target --exclude .git --exclude .superpowers ./ codepit:/root/seesaw-java/src/
for t in java40 open2 open5; do
  rsync -a ${PDE_DUMP:?PDE_DUMP setzen}/$t.trace codepit:/root/seesaw-java/pde-dump/
done
scp -q $D/ladder-t4.txt $D/step.sh codepit:/root/seesaw-java/
LADDER=$D/ladder-t4.txt sh java/bench/jsuite.sh send-ladder
ssh codepit 'ls /root/seesaw-java /root/seesaw-java/stands /root/seesaw-java/pde-dump | tr "\n" " "; echo'
