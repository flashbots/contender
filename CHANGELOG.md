# Changelog

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

> Note: this file did not exist until after `v0.5.6`.

---

## [Unreleased]

### Changed

- **BREAKING**: Replaced `--loops` flag with `--indefinite` flag for spam command ([#395](https://github.com/flashbots/contender/issues/395))
  - Use `--indefinite` to run spam continuously until manually stopped
  - Default behavior (without `--indefinite`) runs spam once
  - Previous `--loops N` functionality replaced with running spamd N times via script
  - Migration: If you used `--loops` without a value for infinite loops, use `--indefinite`. If you used `--loops N` for a specific count, run the command N times or use a wrapper script.

### Added

- **Auto-flush configuration**: New `--cache-flush-interval` flag to control how often (in blocks) the pending transaction cache is flushed to the database ([#395](https://github.com/flashbots/contender/issues/395))
  - Default: 5 blocks
  - Lower values reduce memory usage but increase DB writes
  - Higher values batch more efficiently but use more memory

### Improved

- **Spammer now processes receipts in background**: TxActor automatically flushes pending transaction cache in the background while spamming continues ([#395](https://github.com/flashbots/contender/issues/395))
  - Spamming no longer pauses to collect receipts
  - Cache is automatically flushed at configurable intervals (default: every 5 blocks)
  - More realistic RPC traffic patterns
  - Better performance for long-running spam operations
  - Improved error handling with automatic retry on transient failures
  - Intelligent logging to avoid spam during extended RPC issues

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