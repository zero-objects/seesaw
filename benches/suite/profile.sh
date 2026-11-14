#!/bin/sh
# perf profile of one cell (Linux), by instructions retired in user mode.
#
#   profile.sh <cell> [lines]
#
# Builds the suite with debug info into target/prof and samples one run
# with call graphs. Reports inclusive shares of the engine's and the
# suite's functions, then self shares of all symbols. The profile
# covers the whole process: the measured section is what lies below
# `suite::measure::section` in the inclusive list, model construction,
# rule loading and the fingerprint are outside of it.
set -eu
cell=$1
lines=${2:-40}
cd "$(dirname "$0")/../.."
CARGO_PROFILE_BENCH_DEBUG=1 CARGO_TARGET_DIR=target/prof \
    cargo bench -q --bench suite --no-run 2>/dev/null
bin=$(ls -t target/prof/release/deps/suite-* | grep -v '\.d$' | head -1)
data=$(mktemp /tmp/suite-perf.XXXXXX)
perf record -q --call-graph dwarf,16384 -c 2000003 -o "$data" -e instructions:u \
    "$bin" run "$cell" --samples 1 >/dev/null 2>&1
report() {
    perf report -i "$data" --stdio --no-inline --sort symbol -g none "$@" 2>/dev/null |
        grep -v '^#' | grep -v '^$' | sed 's/suite-[0-9a-f]*  *//; s/libc.so.6  *//' | cut -c1-130
}
echo "== inclusive: $cell"
report --children --percent-limit 1.0 | grep -E 'seesaw_tgg|suite::|blake3::Hasher' | head -"$lines"
echo "== self: $cell"
report --no-children --percent-limit 1.0 | head -"$lines"
rm -f "$data"
