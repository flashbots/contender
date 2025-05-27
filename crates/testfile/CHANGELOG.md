# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.5](https://github.com/flashbots/contender/releases/tag/contender_testfile-v0.1.5) - 2025-05-14

### Fixed

- placeholder logic for > 2 placeholders

### Other

- ci publish ([#215](https://github.com/flashbots/contender/pull/215))
- Adding remote scenarios ([#202](https://github.com/flashbots/contender/pull/202))
- bugfix/tokio task panics ([#187](https://github.com/flashbots/contender/pull/187))
- Feat/more metrics ([#181](https://github.com/flashbots/contender/pull/181))
- engine_ calls to advance chain manually ([#165](https://github.com/flashbots/contender/pull/165))
- drop stalled txs ([#175](https://github.com/flashbots/contender/pull/175))
- op interop scenario ([#136](https://github.com/flashbots/contender/pull/136))
- bugfixes & code organization ([#173](https://github.com/flashbots/contender/pull/173))
- simple scenario + code touchups ([#164](https://github.com/flashbots/contender/pull/164))
- de-duplicate from_pools in TestConfig util fns
- move AgentStore & TestConfig utils into respective impls, fix broken test
- clippy
- various refactors
- add TxType for scenario txs
- implement gas_limit override in generator & testfile
- remove println from unit test
- fmt
- associate RPC_URL with named txs for chain-specific deployments
- support from_pool in create steps
- Merge branch 'main' into add-fmt-clippy-workflows
- make clippy happy
- inject pool signers with generator (TODO: fund them)
- allow tx 'value' field to be fuzzed
- move testfiles into scenarios/
- idiomatic workspace structure
