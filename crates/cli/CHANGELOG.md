# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

- use `miette` for error-printing
- all new error types, cli no longer uses (formerly named) `contender_core::ContenderError`
    - the cli now uses `contender_cli::error::CliError`
    - `CliError` implements `From` for all new error types internal to the cli crate

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
