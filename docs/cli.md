# CLI Reference

Contender provides five top-level commands:

```
contender setup <testfile> [OPTIONS]
contender spam <testfile> [OPTIONS]
contender report [OPTIONS]
contender admin [OPTIONS]
contender db [OPTIONS]
contender campaign <campaign> [OPTIONS]
```

Quick help:
```bash
contender --help
```

## Common flags

**`spam`/`setup`**

- `-r, --rpc-url <URL>` target JSON-RPC endpoint
- `-p, --private-key <HEX>` funder for spammer agents or setup txs

**`spam`**

- `--tps <N>` txs/second (drives agent count)
- `--tpb <N>` txs/block
- `-d, --duration <N>` batches to send before receipt collection
- `-l, --loops [N]` run indefinitely or N times
- `--report` auto-generate a report after spam
- `-e KEY=VALUE` override/insert `[env]` values in a scenario config

### Environment Variables

For some select flags, environment variables can be used in lieu of a flag:

- `RPC_URL`
- `BUILDER_RPC_URL`
- `AUTH_RPC_URL`
- `JWT_SECRET_PATH`
- `CONTENDER_PRIVATE_KEY`
- `CONTENDER_SEED`

> The private key and seed are prefixed with `CONTENDER_` to avoid accidental loss of funds.

**Example:**

instead of `contender spam -r $RPC_URL`:

```bash
export RPC_URL=http://localhost:9545
contender spam ...
```

## Built‑in `spam` subcommands

Contender ships several **builtin scenarios** exposed as `contender spam [...args] <SUBCOMMAND> [...subArgs]`. These are ready to run (no TOML needed) and mirror the structure of file‑based scenarios (create → setup → spam). The exact set may vary by version -- list them with:

```bash
contender spam --help           # shows available SUBCOMMANDs
contender spam <SUBCOMMAND> --help
```

### Common built‑ins (examples)
- **fill-block** — saturate blocks with simple gas‑consuming txs.
- **storage** — heavy `SSTORE` patterns across many keys/slots.
- **eth-functions** — targeted opcode/precompile calls to stress specific code paths.
- **transfers** — many simple ETH transfers (good for baseline throughput).
- **stress** — a composite scenario mixing several patterns.

> Tip: All built‑ins accept the same timing/loop flags as file‑based scenarios:
> `--tps` (per‑second), `--tpb` (per‑block), `-l/--loops`, `-d/--duration`, and env overrides via `-e KEY=VALUE`.

### Usage examples

```bash
# Max‑throughput gas burn
contender spam fill-block -r $RPC_URL --tps 200 -d 5

# Storage write pressure
contender spam storage -r $RPC_URL --tpb 300 -l 50

# Targeted opcode/precompile stress
contender spam eth-functions -r $RPC_URL --tps 100

# Simple baseline
contender spam transfers -r $RPC_URL --tps 50

# Mixed load
contender spam stress -r $RPC_URL --tps 150 -d 3 --report
```

## Campaigns (composite scenarios)

Run multiple scenarios in parallel (optionally in stages) from a campaign TOML:

```bash
contender campaign ./campaigns/base_composite.toml -r $RPC_URL -p $PRIVATE_KEY --report
```

See `docs/campaigns.md` for the file format and execution semantics.
