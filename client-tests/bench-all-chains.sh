#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BENCH_SCRIPT="$SCRIPT_DIR/../scripts/run-contender-bench.sh"

## global vars for run-contender-bench
export SPAM_DURATION=1

# arbitrum
make arbitrum &
sleep 5
CONTENDER_PRIVATE_KEY=0xb6b15c8cb491557369f3c7d2c287b053eb229daa9c22138887752191c9520659 \
RPC_URL=http://localhost:8547 \
BLOCK_GAS_LIMIT=32000000 \
$BENCH_SCRIPT
