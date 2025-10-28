#!/usr/bin/env bash
set -euo pipefail

# --- Config ---
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CONTENDER_BIN="${CONTENDER_BIN:-$SCRIPT_DIR/../target/release/contender}"
RPC_URL="${RPC_URL:-http://127.0.0.1:8545}"
SPAM_DURATION="${SPAM_DURATION:-10}"
BLOCK_GAS_LIMIT="${BLOCK_GAS_LIMIT:-30000000}"

# build contender
cd $SCRIPT_DIR/.. && cargo build --release && cd -

# --- Checks ---
if [[ ! -x "$CONTENDER_BIN" ]]; then
  echo "Error: contender binary not found or not executable: $CONTENDER_BIN"
  echo "Hint: cargo build --release"
  exit 1
fi

echo "== Contender stress matrix =="
echo "RPC: $RPC_URL"
echo "Binary: $CONTENDER_BIN"
echo

# --- Scenarios ---
declare -a SPAM_COMMANDS=(
  "--tps 100 --min-balance 1eth stress"
  "--tps 1000 --min-balance 0.1eth stress"
  "--tps 100 fill-block -g $BLOCK_GAS_LIMIT"
  "--tps 1000 fill-block -g $BLOCK_GAS_LIMIT"
  "--tps 1000 transfers"
  "--tps 5000 transfers"
)

# --- Runner ---
for cmd in "${SPAM_COMMANDS[@]}"; do
  echo ">> contender spam $cmd"
  "$CONTENDER_BIN" spam --rpc-url "$RPC_URL" -d $SPAM_DURATION $cmd
  echo
done

echo "== All runs complete =="
