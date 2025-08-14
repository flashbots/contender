# Contender — Overview

Contender is a high-performance Ethereum transaction spammer and benchmarking tool. It’s built for repeatable load tests against EL clients and networks.

## Key capabilities
- **Config-driven generation** via TOML “scenarios”
- **Timing modes**: per-second (TPS) and per-block (TPB)
- **Seeded fuzzing** for reproducibility
- **SQLite-backed state** for contracts, runs, and reports
- **Extensible** tx generators and callbacks

See also the built-in scenarios in `/scenarios/` for ready-to-run examples.
