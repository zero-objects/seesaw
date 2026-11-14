#!/bin/sh
# perf profile of one cell (Linux): inclusive and self time per symbol.
#
#   profile.sh <cell> [lines]
#
# Builds the suite with debug info into target/prof and samples one run
# with call graphs. The profile covers the whole process, model
# construction and fingerprint included; the measured section is the
# subtree below `suite::measure::section`.
set -eu
cell=$1
lines=${2:-60}
cd "$(dirname "$0")/../.."
CARGO_PROFILE_BENCH_DEBUG=1 CARGO_TARGET_DIR=target/prof \
    cargo bench -q --bench suite --no-run 2>/dev/null
bin=$(ls -t target/prof/release/deps/suite-* | grep -v '\.d$' | head -1)
data=$(mktemp /tmp/suite-perf.XXXXXX)
perf record -q --call-graph dwarf,16384 -F 1500 -o "$data" -e cycles:u \
    "$bin" run "$cell" --samples 1 >/dev/null 2>&1
strip() {
    grep -v '^#' | grep -v '^$' | sed 's/  *suite-[0-9a-f]*  */  /g' | cut -c1-170
}
echo "== inclusive: $cell"
perf report -i "$data" --stdio --children --percent-limit 1.2 -g none 2>/dev/null |
    strip | head -"$lines"
echo "== self: $cell"
perf report -i "$data" --stdio --no-children --percent-limit 1.0 -g none 2>/dev/null |
    strip | head -40
rm -f "$data"
