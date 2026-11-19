#!/bin/sh
# resetup.sh (Mac): /root/seesaw-java auf codepit nach einer neuen Reservierung wieder aufbauen.
# Quellen, pde-Spuren, Stände der Leiter, Treiber, bisherige Ergebnisse. JARs, Messstrecke und
# Rust-Binaries baut step.sh selbst.
set -eu
# WORKTREE: Checkout des Repositorys, PDE_DUMP: Verzeichnis mit den pde-Spuren (*.trace).
W=${WORKTREE:?WORKTREE setzen}
cd $W
ssh codepit 'mkdir -p /root/seesaw-java/src /root/seesaw-java/pde-dump /root/seesaw-java/results'
rsync -a --delete --exclude target --exclude .git --exclude .superpowers ./ codepit:/root/seesaw-java/src/
rsync -a ${PDE_DUMP:?PDE_DUMP setzen}/java40.trace codepit:/root/seesaw-java/pde-dump/
sh java/bench/jsuite.sh send-ladder >/dev/null
scp -q .superpowers/sdd/plan-2026-11-15/step.sh codepit:/root/seesaw-java/step.sh
rsync -a benches/suite/results/2026-11-16-java/ladder/ codepit:/root/seesaw-java/results/
ssh codepit 'ls /root/seesaw-java /root/seesaw-java/stands | tr "\n" " "; echo; wc -l < /root/seesaw-java/results/ladder.tsv'
