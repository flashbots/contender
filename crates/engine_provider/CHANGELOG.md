# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

### Breaking changes

- `NetworkAttributes::new` signature; `rpc_url` type changed from `&str` to `&Url`

## [0.6.0](https://github.com/flashbots/contender/releases/tag/v0.6.0) - 2025-11-25

### (Possibly) breaking changes

These changes should only break your thing if you match the error variants directly. If you just cast it to a string or something you're fine.

- [`AuthProviderError`](./src/error.rs) has several new variants to improve granularity and visibility into the source of an error
    - `AuthProviderError::TransportError` implements `From<alloy::transports::TransportError>` for better RPC error messages

---

> Note: changelogs prior to this point were broken. Please excuse the mess.

## [0.5.5](https://github.com/flashbots/contender/releases/tag/contender_engine_provider-v0.5.5) - 2025-05-14

### Other

- ci publish ([#215](https://github.com/flashbots/contender/pull/215))
- engine_ calls to advance chain manually ([#165](https://github.com/flashbots/contender/pull/165))
