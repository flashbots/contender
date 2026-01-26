#!/usr/bin/env bash

set -euo pipefail

: "${CONTENDER_RPC_URL:?CONTENDER_RPC_URL is required}"
: "${CONTENDER_BIN:?CONTENDER_BIN is required}"
