# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

- replaced timed `--forever` auto-funding with per-batch balance-checked funding: spammer accounts are now refilled at the end of each batch whenever any account drops within 25% of `--min-balance`, avoiding the gradual over-funding the interval-based approach caused on long runs ([#514](https://github.com/flashbots/contender/issues/514))
- (rpc): fix CPU usage bugs ([#527](https://github.com/flashbots/contender/pull/527/changes))

## [0.10.0](https://github.com/flashbots/contender/releases/tag/v0.10.0) - 2026-04-20

- (rpc): accept eth denominations (e.g. "1 eth") for min_balance ([#518](https://github.com/flashbots/contender/pull/518/changes))
- improve campaign/spam logs (particularly when `--report` is passed) ([#508](https://github.com/flashbots/contender/pull/508/changes))
- updated session management to support typestate-based `Contender` lifecycle ([#507](https://github.com/flashbots/contender/pull/507/changes))
  - `ContenderSession.contender` now wraps a `SessionContender` enum (`Uninit` / `Init`)
  - `take_contender` / `put_contender` replaced by typed `take_uninitialized` / `take_initialized` / `put_initialized`
- contender API ([#494](https://github.com/flashbots/contender/pull/494/changes))
  - new `contender server` command; run contender instances via JSON-RPC
  - exposes CLI as a reusable library

## [0.9.1](https://github.com/flashbots/contender/releases/tag/v0.9.1) - 2026-04-01

- auto-fund spammer accounts periodically when running with `--forever` to prevent ETH depletion; refund interval is derived from `--min-balance`, `get_max_spam_cost()`, and `--tps`/`--tpb` ([#502](https://github.com/flashbots/contender/pull/502/changes))
- add `--time-to-inclusion-bucket` flag to configure histogram bucket size in reports ([#498](https://github.com/flashbots/contender/pull/498/changes))
- move default data dir from `~/.contender` to `$XDG_STATE_HOME/contender` (defaults to `~/.local/state/contender`), with automatic migration of existing data ([#460](https://github.com/flashbots/contender/issues/460/changes))
- organize `--help` output into logical sections for `spam` and `campaign` flags ([#408](https://github.com/flashbots/contender/issues/408))
- bugfix: only retry recoverable errors in `init_scenario` (within spam), allow CTRL-C to terminate it ([#503](https://github.com/flashbots/contender/pull/503/changes))
- sort insufficient balances by address for deterministic funding ([#491](https://github.com/flashbots/contender/pull/491/changes))
- support retries on `init_scenario` failures ([#496](https://github.com/flashbots/contender/pull/496/changes))
- add live spam progress report logs, enabled with `--report-interval` ([#493](https://github.com/flashbots/contender/pull/493/changes))

## [0.9.0](https://github.com/flashbots/contender/releases/tag/v0.9.0) - 2026-03-17

- added `--send-raw-tx-sync` flag to `spam` and `campaign` commands ([#459](https://github.com/flashbots/contender/pull/459/changes))
- changed internal erc20 defaults (didn't match cli defaults) ([#443](https://github.com/flashbots/contender/pull/443/changes))
- added chainlink scenario to repo scenarios ([#446](https://github.com/flashbots/contender/pull/446/changes))
- use `std::path::Path` instead of `str` where applicable, add data_dir arg to enable custom data dir at runtime ([453](https://github.com/flashbots/contender/pull/453/changes))
- add json option to `report` ([#453](https://github.com/flashbots/contender/pull/453/changes))
- added `--scenario-label` flag to deploy and spam the same scenario under different labels ([#456](https://github.com/flashbots/contender/pull/456/changes))
- fix: generate report when `--gen-report` is passed to `spam` ([#457](https://github.com/flashbots/contender/pull/457/changes))
- limit concurrent funding tasks to 25 ([#451](https://github.com/flashbots/contender/pull/451/changes))
- added `contender rpc` subcommand: spam any Ethereum JSON-RPC method at a configurable rate with `--rps` and `-d` flags ([#468](https://github.com/flashbots/contender/pull/468))
  - added `--gen-report` flag to `contender rpc` for HTML report generation with latency histogram and percentile table
- add support for flashblocks time-to-inclusion collection & `--flashblocks-ws-url` ([#465](https://github.com/flashbots/contender/pull/465/changes))

## [0.8.0](https://github.com/flashbots/contender/releases/tag/v0.8.0) - 2026-02-02

### Breaking changes

- removed `--redeploy`, no longer skips contract deployments if previously deployed ([#438](https://github.com/flashbots/contender/pull/438))

## [0.7.4](https://github.com/flashbots/contender/releases/tag/v0.7.4) - 2026-01-27

- revised campaign spammer ([#427](https://github.com/flashbots/contender/pull/427/files))

## [0.7.3](https://github.com/flashbots/contender/releases/tag/v0.7.3) - 2026-01-20

- added `--infinite` flag to campaigns; loops campaigns indefinitely ([#420](https://github.com/flashbots/contender/pull/420/files))
- added `--accounts-per-agent` flag to setup, fix campaign seeds ([#425](https://github.com/flashbots/contender/pull/425/files))
- improved `--min-balance` warning log formatting ([#421](https://github.com/flashbots/contender/pull/421/files))

## [0.7.2](https://github.com/flashbots/contender/releases/tag/v0.7.2) - 2026-01-14

- bugfix: deploy contracts before attempting to estimate spam cost ([#416](https://github.com/flashbots/contender/pull/416/files))

## [0.7.1](https://github.com/flashbots/contender/releases/tag/v0.7.1) - 2026-01-09

- removed flag: `spam --timeout` ([#410](https://github.com/flashbots/contender/pull/410/files)) (but later replaced it so as to not break CI workflows using it)
- add `admin contract-address` subcommand ([#412](https://github.com/flashbots/contender/pull/412/files))
- `--timeout` is being deprecated, but is left intact to prevent breaking CI workflows that use contender cli

## [0.7.0](https://github.com/flashbots/contender/releases/tag/v0.7.0) - 2026-01-05

- cli is now solely responsible for intercepting CTRL-C signals ([#404](https://github.com/flashbots/contender/pull/404/files))
  - to shutdown background tasks, we rely on [`CancellationToken`s](https://docs.rs/tokio-util/latest/tokio_util/sync/struct.CancellationToken.html)
  - we no longer require two-phase cancellation (CTRL-C once to stop spamming, CTRL-C again to stop result collection)
    - result collection happens async, so when the user cancels, most results will have already been collected
    - stopping quickly is a better UX than two-phase
- reduced verbosity of logs ([#406](https://github.com/flashbots/contender/pull/406/files))
  - logs now only show source file paths and line numbers when `debug` (or higher) is used in RUST_LOG
- new spam flag `--gas-price` manually sets gas price & disables basefee tracking ([#400](https://github.com/flashbots/contender/pull/400/files))

### Breaking changes

- `commands::spam::spam` removes the `&mut TestScenario` param, creates a `TestScenario` from `spam_args` instead
- `SendSpamCliArgs` replaces `--loops [NUM_LOOPS]` (optional `usize`) with `--forever` (`bool`)
- some functions are moved from `utils` to `commands::spam`
- `commands::spamd` has been deleted (it was just a junk wrapper for `spam`)

## [0.6.0](https://github.com/flashbots/contender/releases/tag/v0.6.0) - 2025-11-25

- support new ENV vars ([#376](https://github.com/flashbots/contender/pull/376))
- add `--skip-setup` flag ([#377](https://github.com/flashbots/contender/pull/377))
- all new error types, cli no longer uses (formerly named) `contender_core::ContenderError` ([#378](https://github.com/flashbots/contender/pull/378))
    - use `miette` for error-printing
    - the cli now uses `contender_cli::error::CliError`
    - `CliError` implements `From` for all new error types internal to the cli crate
- add `--rpc-batch-size` flag to support batch-sending txs ([#381](https://github.com/flashbots/contender/pull/381))

## [0.5.6](https://github.com/flashbots/contender/releases/tag/v0.5.6) - 2025-10-10

- erc20: fuzz recipient address ([#366](https://github.com/flashbots/contender/pull/366))
- add reclaim-eth admin subcommand ([#363](https://github.com/flashbots/contender/pull/363))

---

> Note: changelogs prior to this point were broken. Please excuse the mess.

## [0.5.5](https://github.com/flashbots/contender/releases/tag/contender_cli-v0.5.5) - 2025-05-14

### Added

- add timer warning on contract deployment ([#179](https://github.com/flashbots/contender/pull/179))

### Fixed

- fix spam cost estimate bug ([#188](https://github.com/flashbots/contender/pull/188))
- fix ugly casts
- fix warnings
- fix
- fix providers in tests
- fix erroneous clone
- fix subtraction underflow in heatmap
- fix broken test
- fix util test
- fix slot index bug in heatmap
- fix erroneous panic, improve funding error logs

### Other

- ci publish ([#215](https://github.com/flashbots/contender/pull/215))
- Feat/reports w runtime params ([#213](https://github.com/flashbots/contender/pull/213))
- Build other charts even w ([#214](https://github.com/flashbots/contender/pull/214))
- Feat/runtime param help ([#204](https://github.com/flashbots/contender/pull/204))
- consolidate spamd ([#211](https://github.com/flashbots/contender/pull/211))
- Adding remote scenarios ([#202](https://github.com/flashbots/contender/pull/202))
- add debug log for failed provider calls ([#200](https://github.com/flashbots/contender/pull/200))
- Feat/env vars as cli args ([#189](https://github.com/flashbots/contender/pull/189))
- Feature/174 admin command ([#180](https://github.com/flashbots/contender/pull/180))
- Added default RPC value as http://localhost:8545 ([#196](https://github.com/flashbots/contender/pull/196))
- bugfix/tokio task panics ([#187](https://github.com/flashbots/contender/pull/187))
- Feat/more metrics ([#181](https://github.com/flashbots/contender/pull/181))
- refactor faulty conditional preventing percentages > 100 ([#186](https://github.com/flashbots/contender/pull/186))
- build example report in CI ([#185](https://github.com/flashbots/contender/pull/185))
- engine_ calls to advance chain manually ([#165](https://github.com/flashbots/contender/pull/165))
- quality-of-life fixes ([#178](https://github.com/flashbots/contender/pull/178))
- gas price adder & priority fee bugfix ([#176](https://github.com/flashbots/contender/pull/176))
- drop stalled txs ([#175](https://github.com/flashbots/contender/pull/175))
- bugfixes & code organization ([#173](https://github.com/flashbots/contender/pull/173))
- upgrade alloy ([#172](https://github.com/flashbots/contender/pull/172))
- simplify util functions ([#171](https://github.com/flashbots/contender/pull/171))
- spamd ([#170](https://github.com/flashbots/contender/pull/170))
- tx observability, DB upgrades ([#167](https://github.com/flashbots/contender/pull/167))
- simple scenario + code touchups ([#164](https://github.com/flashbots/contender/pull/164))
- log request id w/ hash when calling sendRawTransaction ([#161](https://github.com/flashbots/contender/pull/161))
- update slot-conflict scenario's fn params, more verbose logs
- clippy
- parallelize block retrieval in report
- parallelize trace retrieval in report command
- switch block type in report to Any
- improve log for funding txs
- add estimate test & general cleanup
- estimate setup cost using anvil
- nitpicking verbiage
- clippy
- various refactors
- add TxType for scenario txs
- clippy + cleanup
- clippy
- add flag to skip deploy prompt in 'run' command
- implement gas_limit override in generator & testfile
- remove unnecessary typecasts
- fetch report fonts from CDN, delete font files
- update header styles
- add fonts
- Change background color
- make charts white
- add deadpine styles to html template
- cleanup nits
- prevent divide-by-zero errors before they happen in spammer
- fund accounts before creating scenario
- add scenario_name to runs table, use in report
- add metadata to report command args
- limit # axis labels to prevent crowded text
- remove default trace decoder (unnecessary & not always supported), add page breaks in report template
- clippy
- make tests parallelizable, take db path as args in db functions
- Merge branch 'main' into feat/db-cli
- error before returning from heatmap.build if no trace data collected
- improve ContenderError::with_err, handle trace failure
- reorganize report module into submodules
- clippy
- update heatmap title
- update template title
- clean up chart styling
- open repot in web browser when it's finished
- generate simple HTML report
- update chart bg colors
- add tx-gas-used chart, cleanup logs
- add time-to-inclusion chart
- put charts in chart module, add gasUsedPerBlock chart
- DRY filenames for charts in report
- simplify & improve cache file handling in report
- save heatmap to reports dir
- DRY data file paths
- cleanup heatmap margins
- add axis labels
- properly label axes
- add legend title to heatmap
- add color legend to heatmap
- cleanup logs, improve heatmap color
- draw simple heatmap (WIP; needs appropriate labels)
- convert heatmap data into matrix (for plotting later)
- cleanup
- add heatmap builder (WIP; collects data but doesn't render)
- simplify args
- add tx tracing to report
- support multiple run_ids in report command
- simplify report further (remove filename option)
- simplify 'report' command
- factor out duration from get_max_spam_cost
- before spamming, error if acct balance l.t. total spam cost
- add post-setup log
- remove unnecessary vec
- add test for fund_accounts: disallows funding early if sender has insufficient balance
- check funder balance is sufficient to fund all accounts before funding any
- clippy
- remove "signers per pool" from setup
- num_accounts = txs_per_period / agents.len()
- associate RPC_URL with named txs for chain-specific deployments
- improve logs from common errors in spam & setup
- add -r flag to spam; runs report and saves to filename passed by -r
- create, save, and load user seed: ~/.contender/seed
- clippy
- save DB to ~/.contender/
- save report to ~/.contender/
- export report to report.csv by default
- cleanup
- cleanup
- support from_pool in create steps
- remove debug log
- clean up logs
- use same default seed for both setup & spam
- (WIP) support from_pool in setup steps; TODO: scale requests by #accts
- cleanup db invocations
- clippy
- move subcommand definitions out of main.rs, into individual mods
- remove timeout, add env var to fill blocks up to a percent
- make clippy happy
- replace Into impl with From
- cleanup doc comments, fix num_txs bug in run db
- add --num-txs to run command
- add termcolor to cli, make prompt orange
- organize db, modify templater return types, prompt user to redeploy on fill-blocks
- read block gas limit from rpc
- spam many txs in fill-block
- rename file
- add 'run' command; runs builtin scenarios
- Merge branch 'main' into add-fmt-clippy-workflows
- remove unnecessary struct member; more dry
- scale EOAs for timed spammer as well
- DRY off
- drop the '2' suffix from new spammers; old ones deleted
- add new timedSpammer using Spammer trait
- add new Spammer trait, replace blockwise spammer
- differentiate seed using pool name, fix account index bug
- cleanup comments & clones
- use RandSeed to generate agent signer keys
- fund pool accounts w/ user account at spam startup
- inject pool signers with generator (TODO: fund them)
- idiomatic workspace structure
