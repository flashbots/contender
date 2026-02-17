# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

- added contender version to bottom of reports ([#452](https://github.com/flashbots/contender/pull/452/changes))
- use `std::path::Path` instead of `str` where applicable ([453](https://github.com/flashbots/contender/pull/453/changes))
- clean up html report UI, support batched `eth_sendRawTransaction` latency metrics ([#455](https://github.com/flashbots/contender/pull/455/changes))

## [0.8.1](https://github.com/flashbots/contender/releases/tag/v0.8.1) - 2026-02-09

- filter/condense heatmap input to prevent performance degradation ([#443](https://github.com/flashbots/contender/pull/443/changes))

## [0.6.0](https://github.com/flashbots/contender/releases/tag/v0.6.0) - 2025-11-25

### Breaking changes

- new [`Error`](./src/error.rs) type replaces usage of `contender_core::Error`

---

> Note: changelogs prior to this point were broken. Please excuse the mess.

## [0.5.5](https://github.com/flashbots/contender/releases/tag/contender_report-v0.5.5) - 2025-05-14

### Added

- moved `report` from cli to its own crate
  - can now be used as a lib by other projects
