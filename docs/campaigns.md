# Campaigns (Composite / Meta-Scenarios)

Campaigns let you run multiple existing scenarios in parallel, optionally in sequential stages, to mimic real network mixes or replay traffic profiles.

## File format

```toml
name = "composite-example"
description = "Simple, Stress, and Reverts traffic mix"

[setup]
scenarios = [
  "scenario:simple.toml",
  "scenario:stress.toml",
  "scenario:reverts.toml",
]

[spam]
mode = "tps"        # or "tpb"
tps = 20            # default rate if a stage omits one
duration = 600      # default duration (seconds if tps, blocks if tpb)
seed = 42           # optional; falls back to CLI --seed or seed file

[[spam.stage]]
name = "steady"
duration_secs = 600
  [[spam.stage.mix]]
  scenario  = "scenario:simple.toml"
  share_pct = 95.0
  [[spam.stage.mix]]
  scenario  = "scenario:stress.toml"
  share_pct = 4.8
  [[spam.stage.mix]]
  scenario  = "scenario:reverts.toml"
  share_pct = 0.2
```

- `mode`: `tps` (per-second) or `tpb` (per-block). Stages can override rate/duration; otherwise they inherit from `[spam]`.
- `duration` at `[spam]` is a **default per-stage** duration, not a total campaign time. Each stage runs for its own duration (seconds if `tps`, blocks if `tpb`), then the next stage starts.
- `share_pct`: scenario weight inside a stage; shares are normalized and rounded, and the last entry absorbs rounding drift to preserve the target rate.
- `[setup].scenarios`: run once, in order, before spamming. Uses the standard `setup` logic for each referenced scenario file.

### Stage basics
- Stages run **sequentially**. Each stage inherits `mode`/`tps`/`duration` from `[spam]` unless the stage overrides them.
- Each stage performs its own setup/init (funding, deploy/config for builtins, scenario init), then starts its spammers at the resolved rate/mix.
- Within a stage, we spin up one spammer per `mix` entry at the computed per-scenario rate; they share a DB handle and run id.
- The next stage starts only after the previous one completes its **stage duration** (seconds for `tps`, blocks for `tpb`). Campaign duration is the sum of stage durations.
- Rates and shares are recomputed per stage, so you can ramp traffic up/down or change blends across time slices.

### Validation
- You must provide either `[[spam.stage]]` entries **or** a shorthand `[spam]` + `[[spam.mix]]` with `spam.duration`.
- If `spam.stage` is present, `spam.mix` is rejected (prefer explicit stages).
- Each stage needs a duration (seconds for `tps`, blocks for `tpb`); if omitted, the `[spam].duration` default is used.
- Mix entries must be non-empty and share percentages must sum to a positive number (they are normalized automatically).

### Shorthand single-stage form
If you omit `[[spam.stage]]` and instead set `spam.duration` plus `[[spam.mix]]`, Contender builds a single implicit stage named `steady`:
```toml
[spam]
mode = "tps"
tps = 20
duration = 600
seed = 42

[[spam.mix]]
scenario  = "scenario:simple.toml"
share_pct = 95.0
[[spam.mix]]
scenario  = "scenario:stress.toml"
share_pct = 4.8
[[spam.mix]]
scenario  = "scenario:reverts.toml"
share_pct = 0.2
```
This is equivalent to writing a single explicit `[[spam.stage]]` named `steady` with the same rate/duration and mix.

### Multi-stage example
See `campaigns/staged-example.toml` for a two-stage campaign that warms up at a lower TPS, then ramps to a steady-state mix.

## CLI usage

Preferred: new subcommand.
```bash
contender campaign ./campaigns/composite.toml \
  -r $RPC_URL -p $PKEY --pending-timeout 12 --rpc-batch-size 0
```

Flags mirror `spam` where they make sense:
- Connection/auth: `--rpc-url`, `--priv-key/-p`, `--builder-url`, JWT/auth flags via `ScenarioSendTxs` options.
- Funding/runtime: `--pending-timeout`, `--accounts-per-agent`, `--rpc-batch-size`, `--ignore-receipts`, `--optimistic-nonces`, `--timeout`, `--report`.
- Setup controls: `--redeploy`, `--skip-setup` (mutually exclusive).

## Reporting

- Per-run: `contender report [-i <last_run_id> --preceding-runs N]`
- Campaign summary: `contender report --campaign [<campaign_id>]` (alias: `--campaign-id`)
  - If `<campaign_id>` is omitted, the latest campaign is used.
  - Generates per-run HTML for all runs in the campaign.
  - Writes `campaign-<campaign_id>.html` and `campaign-<campaign_id>.json` under `~/.contender/reports/` with links, aggregate metrics, and per-stage/per-scenario rollups.
  - If you pass `--report` to `contender campaign ...`, contender will also generate a report for the run-id range at the end of the campaign.
  - If transaction logs are incomplete for any run (e.g., tracing/storage gaps), the campaign report will use stored run metadata for totals/durations and will display a notice; error counts may be under-reported in that case.
- When a stage has multiple `[[spam.stage.mix]]` entries, do not combine it with `--override-senders`; using a single sender across mixes is rejected because it would cause nonce conflicts.

## Execution semantics

1) **Setup**: load each scenario in `[setup].scenarios` and run its setup once (reuse existing setup command).
2) **Stages**: for each `[[spam.stage]]`
   - Resolve stage mode/rate/duration from stage or `[spam]` defaults.
   - Compute per-scenario rates: `scenario_rate = round(total_rate * share_pct/100)`, last entry fixed to hit the total.
   - Spawn one spammer per scenario in the stage, sharing a common `run_id` and database handle.
   - Stage ends after `duration` seconds/blocks.
3) **Reporting**: if `--report` is set, generate a report for all campaign runs after the final stage.

Run metadata now records `campaign_name` and `stage_name` alongside the scenario label (`campaign:<name>::<stage>`), so reports and DB exports can distinguish composite runs.

