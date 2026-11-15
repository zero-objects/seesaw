#!/bin/sh
# gate2.sh <teil>: weitere Pruefungen des Zweigs perf/select, jeder Teil unter 5 Minuten.
set -u
cd /root/seesaw-sel/sel/src || exit 1
sum() { awk '/^test result/ {p+=$4; f+=$6; i+=$8; g++} END {print "groups=" g, "passed=" p, "failed=" f, "ignored=" i}' "$1"; }
bad() { grep -E 'FAILED|panicked at|^error' -A8 "$1" | head -40; }
case $1 in
  r188)
    RUSTFLAGS='-D warnings' CARGO_TARGET_DIR=target/r188 timeout 280 cargo +1.88.0 test >/tmp/sel-188.log 2>&1; echo "R188_TEST_RC=$? $(sum /tmp/sel-188.log)"; bad /tmp/sel-188.log ;;
  r188b)
    RUSTFLAGS='-D warnings' CARGO_TARGET_DIR=target/r188 cargo +1.88.0 clippy --all-targets -- -D warnings >/tmp/sel-188c.log 2>&1; echo "R188_CLIPPY_RC=$?"; grep -E '^(warning|error)' -A10 /tmp/sel-188c.log | head -30
    RUSTFLAGS='-D warnings' CARGO_TARGET_DIR=target/r188 cargo +1.88.0 check --all-targets --features low_memory,parallel_seed >/tmp/sel-188f.log 2>&1; echo "R188_CHECK_BEIDE_RC=$?"; grep -E '^(warning|error)' -A10 /tmp/sel-188f.log | head -30 ;;
  misc)
    cargo +1.88.0 check --lib --target wasm32-unknown-unknown >/tmp/sel-wasm.log 2>&1; echo "WASM_RC=$?"; grep -E '^(warning|error)' -A6 /tmp/sel-wasm.log | head -20
    cargo +1.88.0 check --lib --target wasm32-unknown-unknown --features low_memory >/tmp/sel-wasm2.log 2>&1; echo "WASM_LOW_MEMORY_RC=$?"; grep -E '^(warning|error)' -A6 /tmp/sel-wasm2.log | head -20
    cargo +1.88.0 check --lib --target wasm32-unknown-unknown --features parallel_seed >/tmp/sel-wasm3.log 2>&1; echo "WASM_PARALLEL_SEED_RC=$?"; grep -E '^(warning|error)' -A6 /tmp/sel-wasm3.log | head -20
    RUSTDOCFLAGS='-D warnings' CARGO_TARGET_DIR=target/doc-check cargo doc --no-deps >/tmp/sel-doc.log 2>&1; echo "DOC_RC=$?"; grep -E '^(warning|error)' -A8 /tmp/sel-doc.log | head -24
    RUSTDOCFLAGS='-D warnings' CARGO_TARGET_DIR=target/doc-check cargo doc --no-deps --features low_memory,parallel_seed >/tmp/sel-doc2.log 2>&1; echo "DOC_BEIDE_RC=$?"; grep -E '^(warning|error)' -A8 /tmp/sel-doc2.log | head -24
    CARGO_TARGET_DIR=target/pub timeout 200 cargo publish --dry-run --allow-dirty >/tmp/sel-pub.log 2>&1; echo "PUBLISH_DRY_RUN_RC=$?"; grep -E 'Packaged|warning|error|aborting' /tmp/sel-pub.log | head -8 ;;
  trace)
    CARGO_TARGET_DIR=target/tracetest timeout 250 cargo test --features perf_trace >/tmp/sel-trace.log 2>&1; echo "PERF_TRACE_RC=$? $(sum /tmp/sel-trace.log)"; bad /tmp/sel-trace.log ;;
  mvn*)
    cd java || exit 1
    case $1 in mvn1) arg="";; mvn2) arg="-DargLine=-Dseesaw.anchorIndex=false";; mvn3) arg="-DargLine=-Dseesaw.checks=true";; esac
    rm -rf target/surefire-reports
    timeout 280 mvn -q -B test $arg >/tmp/sel-$1.log 2>&1; echo "MVN_RC=$? [$arg]"
    find target/surefire-reports -name '*.txt' 2>/dev/null | xargs grep -h 'Tests run' 2>/dev/null | awk -F'[:,]' '{r+=$2; f+=$4; e+=$6; s+=$8} END {print "MVN run=" r, "failures=" f, "errors=" e, "skipped=" s}'
    grep -E 'ERROR|FAIL' /tmp/sel-$1.log | head -8 ;;
  be1)
    export CARGO_TARGET_S390X_UNKNOWN_LINUX_GNU_LINKER=s390x-linux-gnu-gcc CARGO_TARGET_S390X_UNKNOWN_LINUX_GNU_RUNNER="qemu-s390x -L /usr/s390x-linux-gnu" CARGO_TARGET_DIR=target/s390x
    T=s390x-unknown-linux-gnu
    timeout 280 cargo test --release --target $T --no-run >/tmp/sel-be-build.log 2>&1; echo "BE_BUILD_RC=$?"; grep -E '^error' -A8 /tmp/sel-be-build.log | head -20 ;;
  be2)
    export CARGO_TARGET_S390X_UNKNOWN_LINUX_GNU_LINKER=s390x-linux-gnu-gcc CARGO_TARGET_S390X_UNKNOWN_LINUX_GNU_RUNNER="qemu-s390x -L /usr/s390x-linux-gnu" CARGO_TARGET_DIR=target/s390x
    T=s390x-unknown-linux-gnu
    PROPTEST_CASES=32 timeout 280 cargo test --release --target $T >/tmp/sel-be-tests.log 2>&1; echo "BE_TESTS_RC=$? $(sum /tmp/sel-be-tests.log)"; bad /tmp/sel-be-tests.log ;;
  be3)
    export CARGO_TARGET_S390X_UNKNOWN_LINUX_GNU_LINKER=s390x-linux-gnu-gcc CARGO_TARGET_S390X_UNKNOWN_LINUX_GNU_RUNNER="qemu-s390x -L /usr/s390x-linux-gnu" CARGO_TARGET_DIR=target/s390x
    T=s390x-unknown-linux-gnu
    cargo bench -q --bench suite --target $T --no-run >/tmp/sel-be-bench.log 2>&1; echo "BE_BENCH_BUILD_RC=$?"
    B=$(ls -t target/s390x/$T/release/deps/suite-* | grep -v '\.d$' | head -1)
    for c in langpack/seed/k50/n2000 langpack/add/k50/n2000 langpack/delta-change/k50/n2000/d200 langpack/delta-delete/k50/n2000/d200 f2p/add/f2000 f2p/delta-delete/f2000/d1000; do
      timeout 120 qemu-s390x -L /usr/s390x-linux-gnu $B run $c --samples 1 --label be 2>/dev/null | awk -F'\t' '{printf "s390x  | %s | steps %s | applied %s | nodes %s | %s\n", $2, $8, $9, $10, $11}' || echo "s390x | $c | RUN_FAILED"
      /root/seesaw-sel/bins/sel run $c --samples 1 --label x86 2>/dev/null | awk -F'\t' '{printf "x86_64 | %s | steps %s | applied %s | nodes %s | %s\n", $2, $8, $9, $10, $11}'
    done ;;
  *) echo "unbekannt: $1"; exit 2 ;;
esac
