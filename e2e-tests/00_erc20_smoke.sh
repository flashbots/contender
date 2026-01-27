#!/usr/bin/env bash

set -euo pipefail
: "${CONTENDER_RPC_URL:?CONTENDER_RPC_URL is required}"
: "${CONTENDER_BIN:?CONTENDER_BIN is required}"

"$CONTENDER_BIN" \
  spam \
  --rpc-url "$CONTENDER_RPC_URL" \
  --duration 10 \
  --tps 50 \
  erc20
