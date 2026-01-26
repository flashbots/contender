#!/usr/bin/env bash

set -euo pipefail
"$(dirname "$0")/check_vars.sh"

"$CONTENDER_BIN" \
  spam \
  --rpc-url "$CONTENDER_RPC_URL" \
  --duration 10 \
  --tps 50 \
  erc20
