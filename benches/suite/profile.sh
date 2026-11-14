#!/bin/sh
# perf profile of the measured section of one cell (Linux).
#
#   profile.sh <cell> [lines] [frequency]
#
# Builds the suite with debug info into target/prof and samples one run
# with call graphs. Only samples below `suite::measure::section` are
# reported: first inclusive per symbol, then self time per symbol. The
# percentages are of the whole process.
set -eu
cell=$1
lines=${2:-45}
freq=${3:-3000}
cd "$(dirname "$0")/../.."
CARGO_PROFILE_BENCH_DEBUG=1 CARGO_TARGET_DIR=target/prof \
    cargo bench -q --bench suite --no-run 2>/dev/null
bin=$(ls -t target/prof/release/deps/suite-* | grep -v '\.d$' | head -1)
data=$(mktemp /tmp/suite-perf.XXXXXX)
perf record -q --call-graph dwarf,16384 -F "$freq" -o "$data" -e cycles:u \
    "$bin" run "$cell" --samples 1 >/dev/null 2>&1
report() {
    perf report -i "$data" --stdio --no-inline -x -p 'measure::section' \
        --sort symbol -g none "$@" 2>/dev/null |
        grep -v '^#' | grep -v '^$' | cut -c1-150
}
echo "== inclusive below the section: $cell"
report --children --percent-limit 0.8 | head -"$lines"
echo "== self below the section: $cell"
report --no-children --percent-limit 0.5 | head -"$lines"
rm -f "$data"
