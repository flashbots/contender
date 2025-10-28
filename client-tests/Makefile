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

# ---------- Optimism (OP Stack devnet via Kurtosis) ----------
# Follow official Kurtosis devnet docs. Requires kurtosis installed. :contentReference[oaicite:1]{index=1}
optimism:
	@command -v kurtosis >/dev/null || { echo "kurtosis not found (see OP docs)"; exit 1; }
	kurtosis run github.com/ethereum-optimism/optimism --enclave optimism-devnet

optimism-down:
	kurtosis clean --delete-persistent-data --yes || true

# ---------- Base (official node repo, docker compose) ----------
# Use upstream base/node compose. See README for ports. :contentReference[oaicite:2]{index=2}
base:
	$(call GIT_CLONE_OR_UPDATE,main,base-node,https://github.com/base/node.git)
	cd $(VENDOR_DIR)/base-node && cp -n .env.sepolia .env || true
	cd $(VENDOR_DIR)/base-node && docker compose up -d

base-down:
	cd $(VENDOR_DIR)/base-node && docker compose down -v || true

# ---------- Polygon zkEVM (local stack) ----------
# Docs show: clone, build docker, then test/make run; L2 RPC commonly on 8123. :contentReference[oaicite:3]{index=3}
polygon-zkevm:
	$(call GIT_CLONE_OR_UPDATE,main,zkevm-node,https://github.com/0xPolygonHermez/zkevm-node.git)
	cd $(VENDOR_DIR)/zkevm-node && make build-docker
	cd $(VENDOR_DIR)/zkevm-node/test && make run

polygon-zkevm-down:
	cd $(VENDOR_DIR)/zkevm-node/test && make stop || true

# ---------- Mantle v2 (Bedrock-aligned local L1+L2) ----------
# Repo has ops/ (and ops-bedrock/). Start via make up. :contentReference[oaicite:4]{index=4}
mantle:
	$(call GIT_CLONE_OR_UPDATE,main,mantle-v2,https://github.com/mantlenetworkio/mantle-v2.git)
	cd $(VENDOR_DIR)/mantle-v2/ops && make up

mantle-down:
	cd $(VENDOR_DIR)/mantle-v2/ops && make down || true

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
