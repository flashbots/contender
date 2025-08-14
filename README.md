# Contender

![Test Status](https://github.com/flashbots/contender/actions/workflows/test.yml/badge.svg)
![Lint Status](https://github.com/flashbots/contender/actions/workflows/lint.yml/badge.svg)
[![License](https://img.shields.io/github/license/flashbots/contender)](./LICENSE)

High-performance Ethereum transaction spammer and benchmarking tool.

## 📚 Docs

### 1. Introduction
- [Overview](docs/overview.md)

### 2. Getting Started
- [Installation](docs/installation.md)
- [CLI Reference](docs/cli.md)
- [Example Commands](docs/examples.md)

### 3. Writing Scenarios
- [Scenario File Structure](docs/scenarios.md)
- [Placeholders](docs/placeholders.md)
- [Creating a New Scenario](docs/creating_scenarios.md)

### 4. Advanced Usage
- [Engine API Spamming](docs/engine-api.md)
- [Reports, Database, and Admin Tools](docs/reports-db-admin.md)
- [Using Contender as a Library](docs/library-usage.md)

### 5. Internals
- [Architecture](docs/architecture.md)

---

## 📝 Overview

Contender is a high-performance Ethereum transaction spammer and benchmarking tool, built for repeatable load tests against EL clients and live networks.
It supports both **per-second** (TPS) and **per-block** (TPB) timing, seeded fuzzing for reproducibility, and SQLite-backed state for contracts, runs, and reports.

---

## ⚙️ Prerequisites

- **Rust toolchain** (latest stable)
- **SQLite development headers** (`libsqlite3-dev` on Linux)
- A JSON-RPC endpoint for the target Ethereum node

---

## 🚀 Quick Start

Install:
```bash
cargo install --git https://github.com/flashbots/contender --locked
```

Run a simple spam scenario:
```bash
contender spam --tps 50 -r $RPC_URL fill-block
```

Run a bundled scenario from the repo:
```bash
contender setup scenario:stress.toml -r $RPC_URL -p $PRIVATE_KEY
contender spam  scenario:stress.toml -r $RPC_URL --tps 10 -d 3
```

See [examples](docs/examples.md) for more usage patterns.
