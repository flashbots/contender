# Stream Mode (`spam-stream`)

> **Status:** prototype / draft. This document accompanies the
> initial implementation of `spam-stream`. Expect the CLI surface and the
> stream format to evolve based on review feedback.

## Motivation

Contender is a TPS spammer driven by static scenario TOML files. It cycles
through `[[spam]]` entries, fuzzes args, and sends at a configured rate.

Some use cases need to feed *dynamically discovered* tx specs into the spam
loop. For example, an interop relayer reads message-emitted events on chain A
and needs to execute the corresponding `validateMessage(...)` call on chain B
— with the right access list, calldata, and target. Today these workflows
end up reimplementing rate limiting, signer pools, nonce management, and
receipt tracking outside contender.

Stream mode lets contender act as the "sender" half of those workflows: any
upstream process can pipe JSON tx specs into contender and reuse the existing
agent pools, rate limiter, `tx_actor` receipt tracking, gas-price caching,
and Prometheus latency metrics.

## CLI

```bash
contender spam-stream \
  -r https://chain-b \
  -p $FUNDING_KEY \
  --from <stdin|FILE> \
  --from-pool executors --pool-size 10 \
  --tps 5
```

Key flags:

| Flag | Default | Meaning |
|------|---------|---------|
| `-r, --rpc-url` | `http://localhost:8545` | Target RPC. |
| `-p, --priv-key` | none | Funder key (funds the pool before spam starts). |
| `--from` | `stdin` | `stdin` or a file path. |
| `--from-pool` | `executors` | Pool name. Specs that omit `from`/`from_pool` use this pool. |
| `--pool-size` | `10` | Accounts generated in the pool. |
| `--tps` | `0` | `0` = consume as fast as channel emits. |
| `--min-balance` | `0.01 ETH` (wei) | Min pool-account balance during funding. |
| `--skip-funding` | `false` | Skip pre-spam funding. |
| `--seed` | random | Deterministic pool generation. |

## Stream format

Newline-delimited JSON, one [`FunctionCallDefinition`](../crates/core/src/generator/function_def.rs)
per line. Same field names as scenario TOML.

Minimal:

```json
{"to":"0xdeAD000000000000000000000000000000000000","value":"1 wei","gas_limit":21000}
```

Full (interop-style):

```json
{
  "to": "0x4200000000000000000000000000000000000022",
  "signature": "validateMessage(bytes32)",
  "args": ["0x0102030405060708091011121314151617181920212223242526272829303132"],
  "access_list": [
    {
      "address": "0x4200000000000000000000000000000000000022",
      "storageKeys": ["0x0100000000000000000000000000000000000000000000000000000000000000"]
    }
  ],
  "gas_limit": 200000
}
```

Empty lines and lines beginning with `#` are ignored. Malformed JSON lines
log a warning and the loop continues.

## Architecture

Stream mode does **not** introduce a new spammer trait or a new tx pipeline.
It reuses the existing `TestScenario` machinery and wires a JSON-line reader
to it through an mpsc channel.

```
stdin/file -> reader task -> mpsc<FunctionCallDefinition>
                                       |
                                       v
                              drive_stream loop
                                       |
                                       v
        for each spec:
          scenario.make_strict_call          (Generator trait, resolves from_pool + access_list)
          scenario.config.template_function_call  (Templater, builds TransactionRequest)
          scenario.prepare_tx_request        (assigns nonce, gas limit, signs key from pool)
          scenario.txs_client.send_tx_envelope
          scenario.tx_actor().cache_run_tx   (queues for receipt polling)
```

Code map:

- [`crates/cli/src/commands/spam_stream.rs`](../crates/cli/src/commands/spam_stream.rs)
  is the new subcommand. All logic lives there.
- The scenario starts from an empty `TestConfig`; the executor pool is
  provisioned directly via `AgentStore::add_new_agent` and its signers are
  registered with the scenario, then nonces are synced from the RPC.
- The reader task is a small wrapper around `tokio::io::BufReader::lines()`
  that forwards parsed specs over an mpsc channel and exits on EOF.
- The drive loop honors `--tps` via `tokio::time::interval`.

## Structured output

`spam-stream` writes a structured, newline-delimited JSON event stream to
**stdout** (human-readable logs go to stderr via `tracing`). Each event is a
versioned, tagged envelope so the schema can evolve without breaking
consumers:

```json
{"version":1,"type":"tx_result","idx":0,"tx_hash":"0x...","start_timestamp_ms":1733155200000,"kind":"validate","error":null}
```

- `version` pins the schema (bump on breaking changes).
- `type` discriminates the event kind (currently only `tx_result`).
- One `tx_result` is emitted per input spec after the send attempt; `error` is
  present only when the send RPC call failed.

## Reuse vs. new code

| Existing piece | Reused as-is |
|----------------|--------------|
| `TestScenario` constructor (signer map, nonce sync, txs_client) | yes |
| `Generator::make_strict_call` (resolves `from_pool`, access list, EIP-7702) | yes |
| `Templater::template_function_call` (calldata encoding, access list threading) | yes |
| `TestScenario::prepare_tx_request` (nonce, gas limit, complete_tx_request) | yes |
| Pool generation via `AgentPools::build_agent_store` | yes |
| `TxActorHandle::cache_run_tx` + flush loop (DB writes, receipt polling) | yes |
| `fund_accounts` helper | yes |
| Prometheus latency histograms via Tower middleware | yes (inherited from `TestScenario`) |
| The `Spammer` trait + `TimedSpammer`/`BlockwiseSpammer` | **not** reused |

The reason we skip `TimedSpammer` is that its `on_spam` loop drives ticks
from a pre-loaded `Vec<Vec<ExecutionRequest>>` returned by
`get_spam_tx_chunks`. Stream mode wants a stream-shaped tick: pull one spec,
send one tx. Bolting the channel into `TimedSpammer` would require a generic
`SpamSource` abstraction across the existing spammers. Out of scope for the
prototype; a candidate for a follow-up if the prototype lands.

## Scope of the prototype

In scope:

- `--from stdin|FILE`, `--from-pool`, `--pool-size`, `--tps`, `--priv-key`, `--rpc-url`, `--seed`, `--min-balance`, `--skip-funding`.
- Same `FunctionCallDefinition` schema as scenario TOML (incl. `access_list`,
  `gas_limit`, `signature`, `args`, `value`, `from`, `from_pool`, `kind`).
- Pool funding from `--priv-key` before spam begins.
- Receipt tracking + DB persistence via the existing tx_actor flush loop.
- Graceful CTRL-C and stream EOF handling (drain pending receipts before exit).

Out of scope (future work):

- Bundle support (`[[spam.bundle]]` analogue in the stream).
- Blob transactions (EIP-4844) and authorization transactions (EIP-7702) —
  the `FunctionCallDefinition` fields are deserialized but not exercised in
  prototype tests.
- Fuzzing in stream mode — fuzz happens upstream of the stream.
- Gas-bump / nonce-shift retry logic from the regular spammer's
  `handle_tx_outcome`. The stream loop currently logs send errors and moves
  on; the upstream is expected to resubmit if it cares.
- `--rpc-batch-size`, `--send-raw-tx-sync` integration.
- Recording the run in the `spam_runs` table — stream runs use `run_id = 0`
  and rely on the tx_actor's cache only.
- A generic `SpamSource` trait so `TimedSpammer` can consume a stream too.

## Validation

Unit tests live alongside the implementation:

```
cargo test -p contender_cli spam_stream
```

Smoke test:

```bash
echo '{"to":"0xdeAD000000000000000000000000000000000000","value":"1","gas_limit":21000}' | \
  contender spam-stream -r $RPC -p $FUNDING_KEY --from stdin --tps 1
```

The tx should land on the target chain; the funder needs at least enough ETH
to fund the executor pool.

## Open questions

1. Should stream mode get its own `Spammer` impl in `contender_core` so
   campaigns can reuse it? Today the prototype lives entirely in `cli/`.
   *(Deferred to a follow-up: refactoring the `Spammer` trait is out of scope
   for the prototype.)*
2. ~~Is the JSON spec the right shape, or should we standardize on a tagged
   envelope so we can evolve it later?~~ **Resolved:** the stdout output is now
   a versioned, tagged envelope (`{"version":1,"type":"tx_result",...}`). See
   "Structured output" above.
3. ~~How should errors propagate back to the upstream producer?~~ **Resolved:**
   `spam-stream` emits a structured `tx_result` event per spec on stdout
   (including send errors), in addition to the DB + logs.
4. Should `--tps 0` (drain-as-fast) bound concurrency by pool size, or is
   "one in flight at a time" acceptable for the relayer case? *(Deferred:
   parallel sends judged not worth the effort for the prototype.)*
