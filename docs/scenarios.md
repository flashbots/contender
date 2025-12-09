# Scenarios

A **scenario** defines deployments and calls before and during a spam run.

A "scenario" may refer to a scenario config file (see [scenarios/](../scenarios/)),
or one of contender's built-in scenarios, available via the spam command
(run `contender spam -h` to list them).

### Quickstart

Clone repo and run local scenario files:

```bash
git clone https://github.com/flashbots/contender
cd contender
cargo run -- setup ./scenarios/stress.toml -r $RPC_URL -p $PRIVATE_KEY
cargo run -- spam  ./scenarios/stress.toml -r $RPC_URL --tps 10 -d 3 -p $PRIVATE_KEY
```

### Code Info

Scenario configs are defined concretely for use in the contender CLI by
[`TestConfig`](../crates/testfile/src/test_config.rs#L16), but spammers actually accept a general trait implementation, which define a "scenario" when composed:

```rs
impl PlanConfig<String> + Templater<String> + Send + Sync + Clone
```

*(from [`contender_core::spammer::Spammer`](../crates/core/src/spammer/spammer_trait.rs#L52))*

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

## Builtin Scenarios

Contender has a variety of built-in scenarios that don't require any local TOML files to run.

Run `contender spam -h` to view all available scenarios.

Subcommands have their own CLI flags, not to be confused with `spam`'s flags:

```bash
# list subcommand flags
contender spam -r http://localhost:8545 --tps 20 erc20 -h

# list spam flags
contender spam -r http://localhost:8545 --tps 20 -h erc20
```

Some `spam` flags, such as `--tps` and `--skip-setup` are available to both `spam` and its subcommands (the builtin scenarios), but not all of them.
We can't share all the args across `spam` and its subcommands because of short-name conflicts.

For example:

```bash
contender spam -r http://localhost:8545 --tps 20 \
         erc20 -r 0x70997970C51812dc3A010C7d01b50e0d17dc79C8
```

`-r` in `spam` refers to `--rpc-url`, while in the `erc20` subcommand, it refers to `--recipient`.

However, we do share some flags for convenience and visibility:

```bash
contender spam --tps 20 erc20
# is the same as:
contender spam erc20 --tps 20
```

If you are running a builtin scenario and encounter an issue, don't forget to check `contender spam --help` as well as `contender spam <builtin_scenario> --help`.

## Composite / Meta-Scenarios (Campaigns)

To run multiple scenarios in parallel (with staged mixes), create a campaign TOML and run:

```bash
contender campaign ./campaigns/base_composite.toml -r $RPC_URL -p $PKEY
```

Campaigns reference existing scenario files and specify per-stage mixes and rates. See `docs/campaigns.md` for the full format and examples.
