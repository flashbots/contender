# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
