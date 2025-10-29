#!/usr/bin/env bash
set -euo pipefail

TARGETS=("arbitrum" "op")
if [[ "${1:-}" == "-h" || "${1:-}" == "--help" || "${1:-}" == "help" ]]; then
    echo "Usage: $(basename "$0") [$(IFS=\|; echo "${TARGETS[*]}")]"
    echo "Runs benchmarks for specified chains. If no arguments are given, all targets are run."
    exit 0
fi

RUN_TARGETS=()
for arg in "$@"; do
    RUN_TARGETS+=("$arg")
done

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BENCH_SCRIPT="$SCRIPT_DIR/../scripts/run-contender-bench.sh"

if [[ -n "${CONTENDER_PRIVATE_KEY:-}" ]]; then
    echo "Warning: CONTENDER_PRIVATE_KEY is set. This may interfere with the test. Please unset it before running this script."
fi

cd $SCRIPT_DIR

## global vars for run-contender-bench
export SPAM_DURATION=1

### arbitrum
run_arbitrum() {
    make arbitrum &

    sleep 4

    CONTENDER_PRIVATE_KEY=0xb6b15c8cb491557369f3c7d2c287b053eb229daa9c22138887752191c9520659 \
    RPC_URL=http://localhost:8547 \
    BLOCK_GAS_LIMIT=32000000 \
    $BENCH_SCRIPT

    make arbitrum-down
}

### optimism
run_op() {
    make optimism-detach

    # TODO: replace this with something more sophisticated; read the logs to see if chain is indexing
    sleep 15

    RPC_URL=http://localhost:8547 \
    BLOCK_GAS_LIMIT=60000000 \
    $BENCH_SCRIPT

    make optimism-down
}

### what next?
#


# run tests
if [ ${#RUN_TARGETS[@]} -eq 0 ]; then
    RUN_TARGETS=("${TARGETS[@]}")
fi
for target in "${RUN_TARGETS[@]}"; do
    case "$target" in
        arbitrum)
            run_arbitrum
            ;;
        op)
            run_op
            ;;
        *)
            echo "Unknown target: $target"
            exit 1
            ;;
    esac
done
