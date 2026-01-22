#!/usr/bin/env bash
set -euo pipefail

: "${CONTENDER_RPC_URL:?CONTENDER_RPC_URL is required}"
: "${CONTENDER_BIN:?CONTENDER_BIN is required}"

echo "[00_erc20_smoke] Starting..."

"$CONTENDER_BIN" \
  spam \
  --rpc-url "$CONTENDER_RPC_URL" \
  --tps 50 \
  erc20 \
  --duration 10s

echo "[00_erc20_smoke] OK"
