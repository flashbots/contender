# Scenarios

A scenario defines deployments and calls before and during a spam run.

Clone repo and try built-ins:
```bash
git clone https://github.com/flashbots/contender
cd contender
cargo run -- setup ./scenarios/stress.toml -r $RPC_URL -p $PRIVATE_KEY
cargo run -- spam  ./scenarios/stress.toml -r $RPC_URL --tps 10 -d 3 -p $PRIVATE_KEY
```

## File structure (TOML)
- `[env]` — variables for interpolation
- `[[create]]` — contract deployments
- `[[setup]]` — one-off txs before spamming
- `[[spam]]` — repeated txs during the run
  - Either **bundles** or **single txs**
  - `[[spam.bundle.tx]]` — txs inside a bundle
  - `[spam.tx]` — single transaction
  - Optional `[[...fuzz]]` for randomized args/values

See `/scenarios/` for concrete examples.
