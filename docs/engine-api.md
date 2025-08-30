# Spamming via `engine_` (FCU/GetPayload)

You can trigger block building through the authenticated Engine API.

Add to `setup` / `spam`:
- `--jwt <path>` path to the node's JWT secret
- `--auth <url>` authenticated Engine API URL
- `--fcu` trigger block building

Targeting Optimism? Add `--op`.

Examples:
```bash
# default
cargo run -- spam scenario:stress.toml -r $RPC \
--auth http://localhost:8551 \
--jwt $JWT_FILE \
--fcu --tps 200 -d 5

# with local op-rbuilder
cargo run -- spam scenario:stress.toml -r http://localhost:1111 \
--auth http://localhost:4444 \
--jwt $CODE/rbuilder/crates/op-rbuilder/src/tester/fixtures/test-jwt-secret.txt \
--fcu --op --tps 200 -d 5
```
