# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

### Breaking changes

- new [`Error`](./src/error.rs) type replaces usage of `contender_core::Error`

---

> Note: changelogs prior to this point were broken. Please excuse the mess.

## [0.5.5](https://github.com/flashbots/contender/releases/tag/contender_report-v0.5.5) - 2025-05-14

### Added

- moved `report` from cli to its own crate
  - can now be used as a lib by other projects
