# Changelog

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

> Note: this file did not exist until after `v0.5.6`.

## Unreleased

- added contender version to bottom of reports ([#452](https://github.com/flashbots/contender/pull/452/changes))
- enable custom data dir at runtime ([453](https://github.com/flashbots/contender/pull/453/changes))
- clean up html report UI, support batched `eth_sendRawTransaction` latency metrics ([#455](https://github.com/flashbots/contender/pull/455))
- added `--scenario-label` flag to deploy and spam the same scenario under different labels ([#456](https://github.com/flashbots/contender/pull/456))
- fix: generate report when `--gen-report` is passed to `spam` ([#457](https://github.com/flashbots/contender/pull/457))
- control `SETUP_CONCURRENCY_LIMIT` with env var ([#461](https://github.com/flashbots/contender/pull/461))

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