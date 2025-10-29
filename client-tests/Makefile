# ===== Local L2 Node Test Matrix (idempotent) =====
# Usage:
#   make arbitrum | optimism | base | polygon-zkevm | mantle | linea | starknet-devnet | starknet-juno
#   TARGET_RPC=http://127.0.0.1:8547 make contender
#
# Config
VENDOR_DIR      ?= .vendor
CONTENDER_BIN   ?= ../target/debug/contender

# ---- helpers ----
define GIT_CLONE_OR_UPDATE
	@if [ -d "$(VENDOR_DIR)/$(2)" ]; then \
	  echo ">> Updating $(2)"; \
	  git -C "$(VENDOR_DIR)/$(2)" fetch --all -p; \
	  git -C "$(VENDOR_DIR)/$(2)" reset --hard "origin/$(1)"; \
	else \
	  echo ">> Cloning $(3) -> $(2)"; \
	  mkdir -p "$(VENDOR_DIR)"; \
	  git clone --depth 1 -b "$(1)" "$(3)" "$(VENDOR_DIR)/$(2)"; \
	fi
endef

.PHONY: clean
clean:
	rm -rf $(VENDOR_DIR)


# ---------- Arbitrum (Nitro dev node) ----------
# Official dev node helper (has run-dev-node.sh)
# Repo README shows default RPC at 127.0.0.1:8547. :contentReference[oaicite:0]{index=0}
arbitrum:
	$(call GIT_CLONE_OR_UPDATE,main,nitro-devnode,https://github.com/OffchainLabs/nitro-devnode.git)
	cd $(VENDOR_DIR)/nitro-devnode && ./run-dev-node.sh

arbitrum-down:
	docker rm -f nitro-dev || true


# --- Optimism via Flashbots builder-playground (build + run) ---
SHELL            := /bin/bash
VENDOR_DIR       ?= .vendor
BP_REPO          ?= https://github.com/flashbots/builder-playground
BP_BRANCH        ?= main
BP_DIR           ?= $(VENDOR_DIR)/builder-playground
BP_BIN_DIR       ?= $(BP_DIR)/.bin
BP_BIN           ?= $(BP_BIN_DIR)/builder-playground
LOG_DIR          ?= .logs
PID_DIR          ?= .pids
OP_LOG           ?= $(LOG_DIR)/optimism.log
OP_PID           ?= $(PID_DIR)/optimism.pid
OP_PROFILE       ?= opstack

.PHONY: bpg-prepare bpg-build op-stack op-stack-detach op-stack-down op-stack-status

bpg-prepare:
	@command -v go >/dev/null    || { echo "go not found"; exit 1; }
	@command -v docker >/dev/null|| { echo "docker not found"; exit 1; }
	@if [ -d "$(BP_DIR)" ]; then \
	  echo ">> updating builder-playground"; \
	  git -C "$(BP_DIR)" fetch --all -p && git -C "$(BP_DIR)" reset --hard origin/$(BP_BRANCH); \
	else \
	  echo ">> cloning builder-playground"; \
	  mkdir -p "$(VENDOR_DIR)"; \
	  git clone --depth 1 -b $(BP_BRANCH) $(BP_REPO) "$(BP_DIR)"; \
	fi

# Build a stable binary so we can signal the real process (not the 'go run' wrapper)
bpg-build: bpg-prepare
	@echo ">> building builder-playground binary -> $(BP_BIN)"
	@mkdir -p "$(BP_BIN_DIR)"
	@cd "$(BP_DIR)" && go build -o "$(BP_BIN)" ./

# Foreground: run the binary; Ctrl-C works and tears down containers cleanly
op-stack: bpg-build
	@cd "$(BP_DIR)" && "$(BP_BIN)" cook "$(OP_PROFILE)"

# Detached: run the binary in background, capture *its* PID, tee logs
op-stack-detach: bpg-build
	@mkdir -p "$(LOG_DIR)" "$(PID_DIR)"
	@echo ">> starting builder-playground (detached), logs -> $(OP_LOG)"
	@nohup bash -lc 'cd "$(BP_DIR)"; exec "$(BP_BIN)" cook "$(OP_PROFILE)"' \
	  >>"$(OP_LOG)" 2>&1 & echo $$! >"$(OP_PID)"
	@echo ">> pid=$$(cat "$(OP_PID)")"
	@echo ">> tail -f $(OP_LOG)"

# Stop: send SIGINT (Ctrl-C) to the binary's PID; fall back to TERM if needed
op-stack-down:
	@if [ -f "$(OP_PID)" ]; then \
	  pid=$$(cat "$(OP_PID)"); \
	  if kill -0 $$pid 2>/dev/null; then \
	    echo ">> sending SIGINT to $$pid"; kill -INT $$pid || true; \
	    sleep 2; \
	    if kill -0 $$pid 2>/dev/null; then \
	      echo ">> still running; sending SIGTERM"; kill -TERM $$pid || true; \
	    fi; \
	  else \
	    echo ">> no running process (stale pid file)"; \
	  fi; \
	  rm -f "$(OP_PID)"; \
	else \
	  echo ">> no pid file at $(OP_PID); nothing to stop"; \
	fi

op-stack-status:
	@if [ -f "$(OP_PID)" ]; then \
	  pid=$$(cat "$(OP_PID)"); \
	  if kill -0 $$pid 2>/dev/null; then \
	    echo "op-stack: RUNNING (pid $$pid) — logs: $(OP_LOG)"; \
	  else \
	    echo "op-stack: not running (stale pid file)"; \
	  fi; \
	else \
	  echo "op-stack: not running"; \
	fi

# ---------- Mantle v2 (Bedrock-aligned local L1+L2) ----------
# Repo has ops/ (and ops-bedrock/). Start via makefile. :contentReference[oaicite:4]{index=4}
mantle:
	$(call GIT_CLONE_OR_UPDATE,main,mantle-v2,https://github.com/mantlenetworkio/mantle-v2.git)
	cd $(VENDOR_DIR)/mantle-v2 && make devnet-up-deploy || true

mantle-down:
	cd $(VENDOR_DIR)/mantle-v2 && make devnet-down || true

# ---------- Linea (local dev stack) ----------
# linea-tutorials exposes L2 on 8645 per README table. :contentReference[oaicite:5]{index=5}
linea:
	$(call GIT_CLONE_OR_UPDATE,main,linea-tutorials,https://github.com/Consensys/linea-tutorials.git)
	cd $(VENDOR_DIR)/linea-tutorials && make start-all

linea-down:
	cd $(VENDOR_DIR)/linea-tutorials && make stop-all || true

# ---------- StarkNet ----------
# Devnet single-container (fast local) — default 5050. :contentReference[oaicite:6]{index=6}
starknet-devnet:
	docker run -d --name starknet-devnet -p 5050:5050 shardlabs/starknet-devnet

starknet-devnet-down:
	docker rm -f starknet-devnet || true

# Full node (Juno) — needs ETH WS endpoint; HTTP port 6060. :contentReference[oaicite:7]{index=7}
starknet-juno:
	@if [ -z "$$ETH_NODE" ]; then echo "Set ETH_NODE to an Ethereum WS URL (e.g. wss://...)"; exit 1; fi
	docker run -d --name juno -p 6060:6060 -v $$HOME/juno:/var/lib/juno \
	  nethermind/juno --http --http-port 6060 --http-host 0.0.0.0 \
	  --db-path /var/lib/juno --eth-node $$ETH_NODE

starknet-juno-down:
	docker rm -f juno || true
