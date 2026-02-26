# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.9.0](https://github.com/flashbots/contender/releases/tag/v0.9.0) - 2026-02-23

- use `impl AsRef<Path>` instead of `&str` for `SqliteDb::from_file` ([453](https://github.com/flashbots/contender/pull/453/changes))
  - non-breaking, will accept `&str` or `&Path`

## [0.6.0](https://github.com/flashbots/contender/releases/tag/v0.6.0) - 2025-11-25

### Breaking changes

- new [`Error`](./src/error.rs) replaces usage of `contender_core::Error`

---

> Note: changelogs prior to this point were broken. Please excuse the mess.

## [0.5.5](https://github.com/flashbots/contender/releases/tag/contender_sqlite-v0.5.5) - 2025-05-14

### Other

- ci publish ([#215](https://github.com/flashbots/contender/pull/215))
- Feat/reports w runtime params ([#213](https://github.com/flashbots/contender/pull/213))
- bugfix/tokio task panics ([#187](https://github.com/flashbots/contender/pull/187))
- Feat/more metrics ([#181](https://github.com/flashbots/contender/pull/181))
- engine_ calls to advance chain manually ([#165](https://github.com/flashbots/contender/pull/165))
- quality-of-life fixes ([#178](https://github.com/flashbots/contender/pull/178))
- tx observability, DB upgrades ([#167](https://github.com/flashbots/contender/pull/167))
- add scenario_name to runs table, use in report
- remove redundant &
- add test assertion for wrong named_tx url
- associate RPC_URL with named txs for chain-specific deployments
- organize db, modify templater return types, prompt user to redeploy on fill-blocks
- make clippy happy
- idiomatic workspace structure
