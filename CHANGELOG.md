# Changelog

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

> Note: this file did not exist until after `v0.5.6`.

---

## v0.6.0

Features:

- more env var support ([#376](https://github.com/flashbots/contender/pull/376))
- `--skip-setup flag`, minor UX improvements ([#377](https://github.com/flashbots/contender/pull/377))
- scenarios: add groth16Verify scenario to test onchain proof verification ([#379](https://github.com/flashbots/contender/pull/379))
- spammer: support batching json-rpc eth_sendRawTransaction reqs ([#381](https://github.com/flashbots/contender/pull/381))
- minor UX improvements ([#382](https://github.com/flashbots/contender/pull/382))

Internal changes:

- revamp error handling ([#378](https://github.com/flashbots/contender/pull/378))