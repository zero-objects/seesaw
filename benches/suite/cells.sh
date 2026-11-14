#!/bin/sh
# Runs cells of the performance suite one after the other, each in its
# own process, and appends one line per cell to the TSV files.
#
#   cells.sh <label> <out-dir> <samples> <cell>...
#
# time.tsv:  user CPU over <samples> runs, built without perf_trace.
# trace.tsv: structural counters of one run, built with perf_trace.
# The two builds keep separate target directories.
set -eu
label=$1
out=$2
samples=$3
shift 3
cd "$(dirname "$0")/../.."
mkdir -p "$out"
for cell in "$@"; do
    CARGO_TARGET_DIR=target/time cargo bench -q --bench suite -- \
        run "$cell" --samples "$samples" --label "$label" --tsv "$out/time.tsv"
    CARGO_TARGET_DIR=target/trace cargo bench -q --bench suite --features perf_trace -- \
        run "$cell" --samples 1 --label "$label" --tsv "$out/trace.tsv"
done
