# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

- refactor `utils::{parse_value, parse_value_opt}` to accept numbers or strings for deser ([#518](https://github.com/flashbots/contender/pull/518/changes))
- logs: replaced "SIMULATING SETUP COST" banners with tracing spans ([#520](https://github.com/flashbots/contender/pull/520/changes))
- save errors to db in NilCallback ([#511](https://github.com/flashbots/contender/pull/511/changes))
- refactored `Contender` to use a typestate-based lifecycle instead of the runtime `ContenderState` enum ([#507](https://github.com/flashbots/contender/pull/507/changes))
  - `Contender<D, S, P, State>` is now generic over a `State` parameter (defaults to `Uninitialized`)
  - `initialize(self)` consumes the uninitialized contender and returns `Contender<..., Initialized<...>>`
  - `spam`, `fund_accounts`, `scenario`, and `scenario_mut` are only callable on `Contender<..., Initialized<...>>`
  - added `initialize_and_spam` convenience helper on `Contender<..., Uninitialized>`
  - added `LifecyclePhase` enum and `PhaseMarker` trait for read-only phase introspection
  - new re-exports: `Initialized`, `Uninitialized`, `LifecyclePhase`, `PhaseMarker`
- added `CombinedCallback<A, B>` and `LogCallback::with_callback` builder so custom spam callbacks can inherit `LogCallback`'s tx-caching (and optional FCU) behavior without duplicating its internals ([#326](https://github.com/flashbots/contender/issues/326))

*from [#494](https://github.com/flashbots/contender/pull/494/changes)*:

- Integrated receipt flushing lifecycle
  - Receipt collection and DB persistence are now fully integrated into the orchestrator run loop:
    - `contender_core::spammer::tx_actor::TxActorHandle` now exposes:
      - `await_flush()` to explicitly wait for the spammer's txs to land
      - `restart_flush()` to restart the flush process, which allows the caller to run another spam run independent of the flush state
      - `clear_cache()` to forcefully empty the cache
    - flushing is automatically awaited at the end of spam (timeout still applies)
- Improved `TxActor` lifecycle management
  - `TxActorHandle` now tracks flush completion and exposes control hooks, enabling:
    - deterministic shutdown
    - tighter coordination between send and receipt phases
- Session-aware task execution utilities
  - Added utilities such as `spawn_with_session` and `CURRENT_SESSION_ID` for propagating session context across async tasks.
- `TestScenario::sync_nonces` now checks for `self.should_sync_nonces` so it may be blindly called

### Breaking changes

*from [#507](https://github.com/flashbots/contender/pull/507)*:

- `contender_core::orchestrator::ContenderState` has been removed.
  - Replace `contender.state.scenario()` with `contender.scenario()` (infallible on `Initialized`).
  - Replace `contender.state.scenario_mut()` with `contender.scenario_mut()`.
  - Replace `contender.state.is_initialized()` checks with compile-time enforcement.
- `Contender::new` now returns `Contender<D, S, P, Uninitialized>`.
- `Contender::initialize` now consumes `self` and returns `Result<Contender<D, S, P, Initialized<D, S, P>>>`.
  - Callers must bind the returned value: `let contender = contender.initialize().await?;`
- `Contender::spam` is no longer available on `Contender<..., Uninitialized>` — call `initialize` first.
  - The implicit auto-initialization inside `spam` has been removed.

*from [#494](https://github.com/flashbots/contender/pull/494/changes)*:

- `contender_core::orchestrator::Contender::spam` signature changed.
  - spam now requires an additional parameter: `cancel_token: Option<CancellationToken>`
  - All existing call sites must be updated.
- `contender_core::orchestrator::Contender::spam` completion semantics changed
  - spam now blocks until receipt flushing is complete (via TxActor), rather than returning immediately after transaction submission.
  - This affects any caller relying on:
    - early return semantics
    - custom receipt collection or post-processing immediately after spam()
- `contender_core::test_scenario::TestScenarioParams` change
  - `pending_tx_timeout_secs: u64` has been changed to `pending_tx_timeout: std::time::Duration`
  - Direct struct construction must be updated accordingly.
- Agent stores are now class-aware
  - `AgentStore` and `SignerStore` now preserve role information for generated agents, enabling downstream logic to distinguish deployers, setup senders, and spammers.
  - `SignerStore::new` added an additional `agent_class: AgentClass` parameter
  - `contender_core::agent_controller::AgentStore` added convenience accessors:
    - `spammers()`
    - `deployers()`
    - `setup_senders()`
    - `get_class(&AgentClass)`
  - `crates/core/src/generator/agent_pools.rs` now assigns explicit agent classes when building stores
    - create pools → `AgentClass::Deployer`
    - setup pools → `AgentClass::SetupSender`
    - spam pools → `AgentClass::Spammer`

## [0.9.1](https://github.com/flashbots/contender/releases/tag/v0.9.1) - 2026-04-01

- added `eth_sendRawTransactionSync` support with per-tx `end_timestamp_ms` tracking ([#459](https://github.com/flashbots/contender/pull/459/changes))
- sort agent addresses for deterministic tx arrangement ([#491](https://github.com/flashbots/contender/pull/491/changes))
- harden against transient RPC failures ([#496](https://github.com/flashbots/contender/pull/496/changes))

## [0.9.0](https://github.com/flashbots/contender/releases/tag/v0.9.0) - 2026-03-17

- remove artificial delay in timed spammer to achieve accurate timing ([#454](https://github.com/flashbots/contender/pull/454/changes))
- added `scenario_label` support to apply contract name labels at the DB boundary ([#456](https://github.com/flashbots/contender/pull/456/changes))
- control `SETUP_CONCURRENCY_LIMIT` with env var ([#461](https://github.com/flashbots/contender/pull/461/changes))
- extracted `collect_latency_from_registry()` into `buckets` module for reuse across crates ([#468](https://github.com/flashbots/contender/pull/468/changes))
- add support for flashblocks w/ time-to-inclusion fields ([#465](https://github.com/flashbots/contender/pull/465/changes))

### Breaking changes

- `TestScenarioParams` has a new required field `send_raw_tx_sync: bool`
- `TestScenarioParams` has a new required field `scenario_label: Option<String>`
- `Generator` trait has a new required method `get_scenario_label() -> Option<&str>`
- `Templater` trait methods `find_placeholder_values`, `find_fncall_placeholders`, and `find_create_placeholders` have a new `scenario_label: Option<&str>` parameter

## [0.8.1](https://github.com/flashbots/contender/releases/tag/v0.8.1) - 2026-02-09

- changed internals of TimedSpammer to tick on `tokio::time::interval` rather than using `sleep` (was causing time drift) ([#443](https://github.com/flashbots/contender/pull/443/changes))
  - added benefit: smoother spam output
- bugfix: limit concurrent setup tasks to prevent FD exhaustion ([#447](https://github.com/flashbots/contender/pull/447/changes))

## [0.8.0](https://github.com/flashbots/contender/releases/tag/v0.8.0) - 2026-02-02

- added timeout for send_transaction calls ([#430](https://github.com/flashbots/contender/pull/430/files))
- track nonces internally for create & setup transactions ([#438](https://github.com/flashbots/contender/pull/438/changes))
  - removed contract deployment detection from builtin scenario spam setup
  - also removed `redeploy`

### Breaking changes

- `TestScenario::load_txs` return type changed to support nonce tracking ([#438](https://github.com/flashbots/contender/pull/438/changes))
- trait bounds `S: Seeder` have been changed to `S: SeedGenerator` (`Seeder + SeedValue`) to support internal agent creation ([#439](https://github.com/flashbots/contender/pull/439/changes))

## [0.7.3](https://github.com/flashbots/contender/releases/tag/v0.7.3) - 2026-01-20

- transactions that revert onchain now store error as "execution reverted" DB, rather than NULL ([#418](https://github.com/flashbots/contender/pull/418/files))
- lock-free TxActor & non-blocking tx-sending ([#423](https://github.com/flashbots/contender/pull/423/files))
  - separates tx receipt processing channel from tx cache ingress channel

## [0.7.0](https://github.com/flashbots/contender/releases/tag/v0.7.0) - 2026-01-05

- setup txs are now sent asynchronously ([#390](https://github.com/flashbots/contender/pull/390/files))
- core no longer processes CTRL-C signals ([#396](https://github.com/flashbots/contender/pull/396/files))
  - instead, `TestScenario` uses a `cancel_token` to shut its processes down
  - `cancel_token.cancel()` is triggered by the caller (e.g. the CLI)
- pending txs are now processed asynchronously ([#396](https://github.com/flashbots/contender/pull/396/files), [#404](https://github.com/flashbots/contender/pull/404/files))
  - `cancel_token.cancelled()` terminates ALL bg processes, making shutdown nearly immediate (and one-step, not two like previously)
  - `TxActor` runs receipt-processing internally (automatically, async)
  - `TxActor` adds a new function `update_ctx_target_block` to internally track the target block to collect receipts from
    - and `is_shutting_down` to report whether it will continue processing
  - `TxActorHandle` adds a new function `done_flushing` to track whether it's done emptying the cache internally
  - `TestScenario` added a new function `shutdown` to trigger cancellation on its `CancellationToken`
- scenario files: `value` now supports units (e.g. `value="1 ether"`) ([#388](https://github.com/flashbots/contender/pull/388/files))
  - values without units are still interpreted as wei

### Breaking changes

- `spammer::error::CallbackError::OneshotSend` now requires a string parameter to be passed along with it
- `SpamRunContext` removed `do_quit` (it was an unnecessarily-copied clone of `TestScenario.ctx.cancel_token`)
- `SpamRunContext` removed `get_msg_handler` (replaced with `TestScenario.tx_actor()`)
- `TxActor` changes the signature of `flush_cache`, `dump_cache`, `remove_cached_tx`, `handle_message`
- `TxActorHandle` adds a new function `init_ctx` which must be called before trying to process receipts
- `flush_tx_cache` removed from `TestScenario` (cache is now passively managed)
- `TestScenarioParams` adds a new param `gas_price: Option<U256>` to control gas price override

## [0.6.0](https://github.com/flashbots/contender/releases/tag/v0.6.0) - 2025-11-25

- support groth16 proof verification in fuzzer ([#379](https://github.com/flashbots/contender/pull/379))
- `TestScenario::execute_spam` now supports batch-sending transactions ([#381](https://github.com/flashbots/contender/pull/381))
- speed up funding step ([#382](https://github.com/flashbots/contender/pull/382))

### Breaking changes

**lib**

- `contender_core::ContenderError` is now [`contender_core::Error`](./src/error.rs)
    - it is a completely new error enum, replacing janky & opaque error types such as `SpamError(&'static str, Option<String>)` with concrete variants such as `Runtime(#[from] RuntimeErrorKind)`
    - it implements `From` for all error types in the `contender_core` crate

**db**

- added `type Error` to the `DbOps` trait
    - core db functions now return `Self::Error` instead of `contender_core::Error`
    - it must implement [`Into<DbError>`](./src/db/error.rs)
- `MockDb` now uses [`MockError`](./src/db/mock.rs#L9) instead of `contender_core::Error`
- moved code from `db/mod.rs` into individual modules

**other modules**

Most usage of `contender_core::Error` has been replaced by module-specific error types (`GeneratorError`, `TemplaterError`, `DbError`, etc.).
These all implement `Into<contender_core::Error>`.

- Spam callbacks now return [`CallbackError`](./src/spammer/error.rs).

---

> Note: changelogs prior to this point were broken. Please excuse the mess.

## [0.5.5](https://github.com/flashbots/contender/releases/tag/contender_core-v0.5.5) - 2025-05-14

### Added

- feat/revert toggle ([#177](https://github.com/flashbots/contender/pull/177))

### Fixed

- fix ugly casts
- fix warnings
- fix
- fix providers in tests
- fix merge bug in test_scenario
- fix arg replacement index bug in templater
- fix early-to-address parse bug, re-enable bundles in spamBundles scenario
- fix templater '{_sender} not in DB' bug
- fix ms-sec logging in report
- fix broken tests & faulty logic
- fix erroneous log
- fix agent-less generator behavior
- fix account index bug properly
- fix invalid index bug

### Other

- ci publish ([#215](https://github.com/flashbots/contender/pull/215))
- Feat/reports w runtime params ([#213](https://github.com/flashbots/contender/pull/213))
- Feat/runtime param help ([#204](https://github.com/flashbots/contender/pull/204))
- consolidate spamd ([#211](https://github.com/flashbots/contender/pull/211))
- add debug log for failed provider calls ([#200](https://github.com/flashbots/contender/pull/200))
- Feature/174 admin command ([#180](https://github.com/flashbots/contender/pull/180))
- bugfix/tokio task panics ([#187](https://github.com/flashbots/contender/pull/187))
- Feat/more metrics ([#181](https://github.com/flashbots/contender/pull/181))
- engine_ calls to advance chain manually ([#165](https://github.com/flashbots/contender/pull/165))
- quality-of-life fixes ([#178](https://github.com/flashbots/contender/pull/178))
- gas price adder & priority fee bugfix ([#176](https://github.com/flashbots/contender/pull/176))
- drop stalled txs ([#175](https://github.com/flashbots/contender/pull/175))
- bugfixes & code organization ([#173](https://github.com/flashbots/contender/pull/173))
- upgrade alloy ([#172](https://github.com/flashbots/contender/pull/172))
- simplify util functions ([#171](https://github.com/flashbots/contender/pull/171))
- spamd ([#170](https://github.com/flashbots/contender/pull/170))
- tx observability, DB upgrades ([#167](https://github.com/flashbots/contender/pull/167))
- simple scenario + code touchups ([#164](https://github.com/flashbots/contender/pull/164))
- log request id w/ hash when calling sendRawTransaction ([#161](https://github.com/flashbots/contender/pull/161))
- update slot-conflict scenario's fn params, more verbose logs
- remove redundant param names
- move AgentStore & TestConfig utils into respective impls, fix broken test
- use destructure syntax on TestScenarioParams, fix log verbiage
- accept pre-set gas_limit in setup steps
- use 1 for prio fee
- tighten up setup_cost test
- add estimate test & general cleanup
- improve logs, prevent u256 underflow
- estimate setup cost using anvil
- improve log copypastability
- clippy
- prevent priority-fee error (never higher than gasprice)
- various refactors
- add TxType for scenario txs
- show run_id after terminated spam runs
- Merge branch 'main' into bugfix/ctrl-c-handling
- clippy + cleanup
- add test for gas override
- implement gas_limit override in generator & testfile
- update alloy
- remove unused dep from core
- Merge branch 'main' into dan/add-eth-sendbundle-from-alloy
- Merge branch 'main' into bugfix/spam-funding
- cleanup nits
- prevent divide-by-zero errors before they happen in spammer
- add scenario_name to runs table, use in report
- improve ContenderError::with_err, handle trace failure
- clippy
- remove erroneous parenthesis-removal, replace bad error types in generator::util
- remove erroneous parenthesis-removal, replace bad error types in generator::util
- better debug errors
- better setup failure logs
- flatten struct (tuple) args in fn sig to parse correctly
- before spamming, error if acct balance l.t. total spam cost
- support {_sender} in 'to' address, rename scenarios, use from_pool in spamBundles (prev. spamMe)
- fmt
- Update rand_seed.rs
- fund accounts in blockwise spam test
- remove unnecessary casts
- add test to check number of agent accounts used by spammer
- better error message for missing contract deployments
- associate RPC_URL with named txs for chain-specific deployments
- inject {_sender} with/without 0x prefix depending on whether it's the whole word
- inject {_sender} placeholder with from address
- improve logs from common errors in spam & setup
- add test for agent usage in create steps
- group spam txs by spam step, not account
- support from_pool in create steps
- use eip1559 txs to fund test scenario in tests
- add test for agent signers in setup step
- remove debug log
- use scaled from_pool accounts in setup generator
- (WIP) support from_pool in setup steps; TODO: scale requests by #accts
- clippy
- make CTRL-C handling extra-graceful (2-stage spam termination)
- remove redundant data in gas_limits map key
- clippy
- accurately account gas usage
- make clippy happy
- log gas_used & block_num for each landed tx
- log failed tx hashes
- log gas limit
- intercept CTRL-C to exit gracefully
- don't crash on failed task
- add stress.toml, tweak mempool.toml, remove # from blocknum log
- remove timeout, add env var to fill blocks up to a percent
- organize db, modify templater return types, prompt user to redeploy on fill-blocks
- spam many txs in fill-block
- add 'run' command; runs builtin scenarios
- comment out unused dep (will use soon)
- add default impl for blockwise spammer
- Merge branch 'main' into add-fmt-clippy-workflows
- remove unnecessary struct member; more dry
- remove unused varc
- extend timeout to num_reqs
- scale EOAs for timed spammer as well
- DRY off
- relax timeout, don't crash on error when waiting for callbacks to finish
- cleanup
- drop the '2' suffix from new spammers; old ones deleted
- delete old spammers
- add new timedSpammer using Spammer trait
- add new Spammer trait, replace blockwise spammer
- improve spamMe scenario & blockwise spammer UX
- differentiate seed using pool name, fix account index bug
- cleanup comments & clones
- cleanup logs
- use RandSeed to generate agent signer keys
- fund pool accounts w/ user account at spam startup
- inject pool signers with generator (TODO: fund them)
- db/mod.rs => db.rs
- move bundle_provider to its own crate (should be replaced entirely, anyhow)
- syntax cleanups
- add simple wallet store (unimplemented)
- remove errant panic, improve logs for bad config
- remove unused import
- cleanup, remove unneeded field in example config
- allow tx 'value' field to be fuzzed
- idiomatic workspace structure
