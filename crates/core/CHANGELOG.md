# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

- added timeout for send_transaction calls ([#430](https://github.com/flashbots/contender/pull/430/files))
- track nonces internally for create & setup transactions ([#438](https://github.com/flashbots/contender/pull/438/changes))
  - removed contract deployment detection from builtin scenario spam setup
  - also removed `redeploy`

### Breaking changes

- `TestScenario::load_txs` return type changed to support nonce tracking ([#438](https://github.com/flashbots/contender/pull/438/changes))

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
