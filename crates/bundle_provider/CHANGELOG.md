# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.5.2](https://github.com/flashbots/contender/releases/tag/contender_bundle_provider-v0.5.2) - 2025-05-14

### Other

- ci publish ([#215](https://github.com/flashbots/contender/pull/215))
- bugfix/tokio task panics ([#187](https://github.com/flashbots/contender/pull/187))
- Merge branch 'main' into update-alloy
- update alloy
- remove unused deps
- *(bundle_provider)* rewrite BundleClient to use alloy
- before spamming, error if acct balance l.t. total spam cost
- add new Spammer trait, replace blockwise spammer
- move bundle_provider to its own crate (should be replaced entirely, anyhow)
