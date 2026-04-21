# Changelog

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

> Note: this file did not exist until after `v0.5.6`.

## Unreleased

- replaced interval-based spammer refunding with a per-batch balance check, preventing over-funding on long `--forever` runs ([#514](https://github.com/flashbots/contender/issues/514))

## [0.10.0](https://github.com/flashbots/contender/releases/tag/v0.10.0) - 2026-04-20

- (rpc): accept eth denominations (e.g. "1 eth") for min_balance ([#518](https://github.com/flashbots/contender/pull/518))
- report now includes total/successful/failed transaction counts with failure rate ([#519](https://github.com/flashbots/contender/pull/519))
- core logs: replaced "SIMULATING SETUP COST" banners with tracing spans ([#520](https://github.com/flashbots/contender/pull/520))
- errors are now also saved to the DB when not collecting receipts ([#511](https://github.com/flashbots/contender/pull/511/changes))
- improve campaign/spam logs (particularly when `--report` is passed) ([#508](https://github.com/flashbots/contender/pull/508))
- refactored `Contender` orchestrator to a typestate-based lifecycle — `Contender<Uninitialized>` vs `Contender<Initialized>` — eliminating all runtime lifecycle checks and enforcing correct usage at compile time ([#507](https://github.com/flashbots/contender/pull/507))
- custom spam callbacks can now wrap a `LogCallback` via `LogCallback::with_callback` to inherit the tx-caching and FCU behavior that `contender report` depends on, removing the boilerplate that previously caused custom callbacks to silently break reporting ([#326](https://github.com/flashbots/contender/issues/326))
- added contender server + some supporting (and internally-breaking) changes ([#494](https://github.com/flashbots/contender/pull/494/))

## [0.9.1](https://github.com/flashbots/contender/releases/tag/v0.9.1) - 2026-04-01

- support spamming with `eth_sendRawTransactionSync` with new flag `--send-raw-tx-sync` ([#459](https://github.com/flashbots/contender/pull/459))
- auto-fund spammer accounts periodically when running with `--forever` to prevent ETH depletion ([#502](https://github.com/flashbots/contender/pull/502))
- add `--time-to-inclusion-bucket` flag to configure histogram bucket size in reports ([#498](https://github.com/flashbots/contender/pull/498))
- move default data dir to `$XDG_STATE_HOME/contender` (`~/.local/state/contender`), with automatic migration from legacy `~/.contender` ([#460](https://github.com/flashbots/contender/issues/460))
- organize `--help` output into logical sections for `spam` and `campaign` flags ([#408](https://github.com/flashbots/contender/issues/408))
- bugfix: only retry recoverable errors in `init_scenario` (within spam), allow CTRL-C to terminate it ([#503](https://github.com/flashbots/contender/pull/503))
- harden against RPC failures ([#496](https://github.com/flashbots/contender/pull/496/changes))
- add live spam progress report logs, enabled with `--report-interval` ([#493](https://github.com/flashbots/contender/pull/493/changes))

## [0.9.0](https://github.com/flashbots/contender/releases/tag/v0.9.0) - 2026-03-17

- added `--send-raw-tx-sync` flag to `spam` and `campaign` for `eth_sendRawTransactionSync` support ([#459](https://github.com/flashbots/contender/pull/459))
- limit concurrent funding tasks to 25 ([#451](https://github.com/flashbots/contender/pull/451/changes))
- added contender version to bottom of reports ([#452](https://github.com/flashbots/contender/pull/452/changes))
- enable custom data dir at runtime ([453](https://github.com/flashbots/contender/pull/453/changes))
- clean up html report UI, support batched `eth_sendRawTransaction` latency metrics ([#455](https://github.com/flashbots/contender/pull/455))
- added `--scenario-label` flag to deploy and spam the same scenario under different labels ([#456](https://github.com/flashbots/contender/pull/456))
- fix: generate report when `--gen-report` is passed to `spam` ([#457](https://github.com/flashbots/contender/pull/457))
- control `SETUP_CONCURRENCY_LIMIT` with env var ([#461](https://github.com/flashbots/contender/pull/461))
- add gas quantiles to report ([#464](https://github.com/flashbots/contender/pull/464))
- add support for flashblocks time-to-inclusion collection & `--flashblocks-ws-url` ([#465](https://github.com/flashbots/contender/pull/465/changes))
- added `contender rpc` subcommand for spam-testing arbitrary JSON-RPC methods with latency tracking and HTML report generation ([#468](https://github.com/flashbots/contender/pull/468))

## [0.8.1](https://github.com/flashbots/contender/releases/tag/v0.8.1) - 2026-02-09

- bugfix: fixed internal default erc20 args, made `TimedSpammer` output more regular ([#443](https://github.com/flashbots/contender/pull/443))
- bugfix: limit concurrent setup tasks to prevent FD exhaustion ([#447](https://github.com/flashbots/contender/pull/447))


## [0.8.0](https://github.com/flashbots/contender/releases/tag/v0.8.0) - 2026-02-02

- track nonces internally for create & setup transactions ([#438](https://github.com/flashbots/contender/pull/438))
- bugfix: tolerate failure of `get_block_receipts` ([#438](https://github.com/flashbots/contender/pull/438))

### Breaking changes

- removed `--redeploy`, no longer skips contract deployments if previously deployed ([#438](https://github.com/flashbots/contender/pull/438))
- breaking changes in `contender_core` (see [core changelog](./crates/core/CHANGELOG.md) for details)

## [0.7.4](https://github.com/flashbots/contender/releases/tag/v0.7.4) - 2026-01-27

- revised campaign spammer ([#427](https://github.com/flashbots/contender/pull/427))

## [0.7.3](https://github.com/flashbots/contender/releases/tag/v0.7.3) - 2026-01-20

- transactions that revert onchain now store error as "execution reverted" DB, rather than NULL ([#418](https://github.com/flashbots/contender/pull/418))
- added `--infinite` flag to campaigns; loops campaigns indefinitely ([#420](https://github.com/flashbots/contender/pull/420))
- added `--accounts-per-agent` flag to setup, fix campaign seeds ([#425](https://github.com/flashbots/contender/pull/425))
- improved `--min-balance` warning log formatting ([#421](https://github.com/flashbots/contender/pull/421))
- lock-free TxActor & non-blocking tx-sending ([#423](https://github.com/flashbots/contender/pull/423))

## [0.7.2](https://github.com/flashbots/contender/releases/tag/v0.7.2) - 2026-01-14

- bugfix: deploy contracts before attempting to estimate spam cost ([#416](https://github.com/flashbots/contender/pull/416))

## [0.7.1](https://github.com/flashbots/contender/releases/tag/v0.7.1) - 2026-01-09

- `--timeout` flag from `contender spam` is deprecated, will be removed in a future release
- added `contender admin contract-address` command ([#412](https://github.com/flashbots/contender/pull/412))
- fixed funding-tx collision bug triggered by campaigns ([#413](https://github.com/flashbots/contender/pull/413))

## [0.7.0](https://github.com/flashbots/contender/releases/tag/v0.7.0) - 2026-01-05

- setup txs are now sent asynchronously ([#390](https://github.com/flashbots/contender/pull/390))
- added campaigns: meta-scenario config files ([#389](https://github.com/flashbots/contender/pull/389))
- scenario files now support units in `value` field ([#388](https://github.com/flashbots/contender/pull/388))
- tx cache is now processed async & independent of spammer ([#396](https://github.com/flashbots/contender/pull/396))
- spam gas price can now be locked manually with `--gas-price` ([#400](https://github.com/flashbots/contender/pull/400))
- CLI logs are now less verbose ([#406](https://github.com/flashbots/contender/pull/406))

*Potentially breaking:*

- `--loops [num]` has been replaced with `--forever` (bool)

---

## [0.6.0](https://github.com/flashbots/contender/releases/tag/v0.6.0) - 2025-11-25

Features:

- more env var support ([#376](https://github.com/flashbots/contender/pull/376))
- `--skip-setup flag`, minor UX improvements ([#377](https://github.com/flashbots/contender/pull/377))
- scenarios: add groth16Verify scenario to test onchain proof verification ([#379](https://github.com/flashbots/contender/pull/379))
- spammer: support batching json-rpc eth_sendRawTransaction reqs ([#381](https://github.com/flashbots/contender/pull/381))
- minor UX improvements ([#382](https://github.com/flashbots/contender/pull/382))
- campaign meta-scenarios: new `contender campaign` command and campaign TOML schema for staged parallel mixes

Internal changes:

- revamp error handling ([#378](https://github.com/flashbots/contender/pull/378))
- DB schema bumped to `user_version = 6` to record campaign/stage metadata in runs.
  - If you see a DB version mismatch, export/reset your DB: `contender db export` (optional backup) then `contender db reset` (or `drop`) to recreate with the new schema.
