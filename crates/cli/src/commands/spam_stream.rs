//! `spam-stream` subcommand: read newline-delimited JSON tx specs from stdin
//! or a file, and spam them through the contender pipeline.
//!
//! This is the entry point for the "stream mode" prototype. See
//! `docs/stream-mode.md` for the design note.

use crate::{
    commands::{
        common::{HELP_HEADING_COMMON, HELP_HEADING_PAYLOAD, HELP_HEADING_RUNTIME},
        error::ArgsError,
        Result,
    },
    error::CliError,
    util::fund_accounts,
    LATENCY_HIST as HIST, PROM,
};
use alloy::{
    consensus::TxType,
    network::{AnyTxEnvelope, Ethereum, NetworkTransactionBuilder},
    primitives::{utils::format_ether, U256},
    providers::Provider,
    transports::http::reqwest::Url,
};
use clap::Args;
use contender_core::{
    generator::{
        agent_pools::AgentSpec, seeder::rand_seed::SeedGenerator, templater::Templater,
        types::SpamRequest, FunctionCallDefinition, Generator, PlanConfig, RandSeed,
    },
    spammer::tx_actor::{ActorContext, CacheTx},
    test_scenario::{TestScenario, TestScenarioParams},
    BundleType,
};
use contender_sqlite::SqliteDb;
use contender_testfile::TestConfig;
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    sync::mpsc,
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

/// Default agent-pool name when the user does not specify one.
const DEFAULT_POOL: &str = "executors";
/// Channel buffer between the reader task and the spam loop.
const STREAM_BUFFER: usize = 256;

#[derive(Clone, Debug, Args)]
pub struct SpamStreamCliArgs {
    /// RPC URL to send transactions to.
    #[arg(
        env = "RPC_URL",
        short = 'r',
        long,
        default_value = "http://localhost:8545",
        help_heading = HELP_HEADING_COMMON,
    )]
    pub rpc_url: Url,

    /// Funder private key. Used to fund the pool of executor accounts before spam begins.
    #[arg(
        env = "CONTENDER_PRIVATE_KEY",
        short = 'p',
        long = "priv-key",
        help_heading = HELP_HEADING_COMMON,
    )]
    pub private_key: Option<String>,

    /// Source of the JSON-lines stream: either `stdin` or a file path.
    #[arg(
        long = "from",
        default_value = "stdin",
        help_heading = HELP_HEADING_COMMON,
    )]
    pub from: String,

    /// Pool name used to source signers for each tx. Stream specs that omit
    /// `from`/`from_pool` will default to this pool.
    #[arg(
        long = "from-pool",
        default_value = DEFAULT_POOL,
        help_heading = HELP_HEADING_PAYLOAD,
    )]
    pub from_pool: String,

    /// Number of accounts to generate in the pool.
    #[arg(
        long = "pool-size",
        default_value_t = 10,
        help_heading = HELP_HEADING_PAYLOAD,
    )]
    pub pool_size: usize,

    /// Target transactions per second. `0` means "send as fast as the stream
    /// can be parsed".
    #[arg(
        long,
        default_value_t = 0,
        help_heading = HELP_HEADING_RUNTIME,
    )]
    pub tps: u64,

    /// Minimum balance to keep in each pool account, in wei.
    #[arg(
        long,
        default_value_t = U256::from(10_000_000_000_000_000u128),
        help_heading = HELP_HEADING_RUNTIME,
    )]
    pub min_balance: U256,

    /// Seed for deterministic pool-signer generation.
    #[arg(
        env = "CONTENDER_SEED",
        long,
        help_heading = HELP_HEADING_RUNTIME,
    )]
    pub seed: Option<String>,

    /// Skip funding the executor accounts. Useful when the pool was funded
    /// out-of-band or every spec carries a `from` address with an existing balance.
    #[arg(long, default_value_t = false, help_heading = HELP_HEADING_RUNTIME)]
    pub skip_funding: bool,
}

/// Asynchronously reads JSON lines from `from` (stdin or file path), parses each
/// into a `FunctionCallDefinition`, and forwards it to `tx`. The task exits when
/// EOF is reached or the receiver drops.
pub fn spawn_stream_reader(
    from: &str,
    tx: mpsc::Sender<FunctionCallDefinition>,
) -> Result<tokio::task::JoinHandle<()>> {
    let handle: tokio::task::JoinHandle<()> = if from == "stdin" {
        tokio::spawn(async move {
            let reader = BufReader::new(tokio::io::stdin());
            forward_lines(reader, tx).await;
        })
    } else {
        let path = PathBuf::from(from);
        if !path.exists() {
            return Err(CliError::Args(ArgsError::Custom(format!(
                "stream source file not found: {}",
                path.display()
            ))));
        }
        tokio::spawn(async move {
            match tokio::fs::File::open(&path).await {
                Ok(f) => {
                    let reader = BufReader::new(f);
                    forward_lines(reader, tx).await;
                }
                Err(e) => warn!("failed to open stream source {}: {e}", path.display()),
            }
        })
    };
    Ok(handle)
}

async fn forward_lines<R>(reader: R, tx: mpsc::Sender<FunctionCallDefinition>)
where
    R: tokio::io::AsyncBufRead + Unpin,
{
    let mut lines = reader.lines();
    let mut line_no: u64 = 0;
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                line_no += 1;
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    continue;
                }
                match serde_json::from_str::<FunctionCallDefinition>(trimmed) {
                    Ok(spec) => {
                        if tx.send(spec).await.is_err() {
                            // receiver dropped — stop reading
                            return;
                        }
                    }
                    Err(e) => warn!("stream: skipping malformed line {line_no}: {e}"),
                }
            }
            Ok(None) => return, // EOF
            Err(e) => {
                warn!("stream: read error: {e}");
                return;
            }
        }
    }
}

/// Build a one-step `TestConfig` that references `from_pool`, so the scenario
/// builds an agent store containing that pool with `pool_size` signers.
/// The decoy entry is never executed; we bypass `load_txs` entirely.
fn build_decoy_config(from_pool: &str) -> TestConfig {
    let decoy = FunctionCallDefinition::new("0x0000000000000000000000000000000000000000")
        .with_from_pool(from_pool);
    TestConfig {
        env: None,
        create: None,
        setup: None,
        spam: Some(vec![SpamRequest::Tx(Box::new(decoy))]),
    }
}

/// Drive the stream loop: pull specs, build/sign/send txs, cache in the
/// tx_actor for receipt tracking. Returns when the stream channel closes.
async fn drive_stream<S, P>(
    scenario: &mut TestScenario<SqliteDb, S, P>,
    mut rx: mpsc::Receiver<FunctionCallDefinition>,
    run_id: u64,
    fallback_pool: String,
    tps: u64,
    cancel: CancellationToken,
) -> Result<usize>
where
    S: SeedGenerator + Send + Sync + Clone,
    P: PlanConfig<String> + Templater<String> + Send + Sync + Clone,
{
    // Rate limiter: only ticks when tps > 0.
    let mut ticker = if tps > 0 {
        let period = Duration::from_secs_f64(1.0 / tps as f64);
        let mut int = tokio::time::interval(period);
        int.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        Some(int)
    } else {
        None
    };

    let mut sent: usize = 0;
    let mut idx: usize = 0;
    let placeholder_map = std::collections::HashMap::<String, String>::new();

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                info!("stream cancelled; flushing.");
                break;
            }
            spec = rx.recv() => {
                let Some(mut spec) = spec else {
                    debug!("stream EOF received");
                    break;
                };

                // Apply default pool if the spec doesn't pick one.
                if spec.from.is_none() && spec.from_pool.is_none() {
                    spec.from_pool = Some(fallback_pool.clone());
                }

                // Rate limit (only when --tps > 0).
                if let Some(int) = ticker.as_mut() {
                    int.tick().await;
                }

                match send_one(scenario, &spec, idx, &placeholder_map, run_id).await {
                    Ok(()) => {
                        sent += 1;
                    }
                    Err(e) => warn!("stream: failed to send tx (idx {idx}): {e}"),
                }
                idx += 1;
            }
        }
    }

    Ok(sent)
}

/// Build a single transaction from a stream spec, sign it, send it, and cache
/// it in the tx_actor for receipt tracking.
async fn send_one<S, P>(
    scenario: &mut TestScenario<SqliteDb, S, P>,
    spec: &FunctionCallDefinition,
    idx: usize,
    placeholder_map: &std::collections::HashMap<String, String>,
    _run_id: u64,
) -> Result<()>
where
    S: SeedGenerator + Send + Sync + Clone,
    P: PlanConfig<String> + Templater<String> + Send + Sync + Clone,
{
    // 1. Resolve `from`/`from_pool` and access list against the scenario's
    //    agent store + templater. This produces a strict FunctionCallDefinition.
    let strict = scenario
        .make_strict_call(spec, idx)
        .map_err(contender_core::Error::Generator)?;

    // 2. Render the strict definition into a TransactionRequest (encodes
    //    calldata, threads access_list, sets value/gas_limit).
    let tx_req = scenario
        .get_templater()
        .template_function_call(&strict, placeholder_map)
        .map_err(contender_core::Error::Templater)?;

    // 3. Fetch a gas price and assign nonce/gas-limit + sign.
    let gas_price = scenario.rpc_client.get_gas_price().await?;
    let (prepared, wallet) = scenario.prepare_tx_request(&tx_req, gas_price, 0).await?;
    // Build & sign via the alloy Ethereum network. The op-alloy-network re-export
    // can create trait-resolution ambiguity, so we fully-qualify the trait
    // method and convert the error string-ly instead of relying on From.
    let envelope =
        <alloy::rpc::types::TransactionRequest as NetworkTransactionBuilder<Ethereum>>::build(
            prepared, &wallet,
        )
        .await
        .map_err(|e| CliError::Args(ArgsError::Custom(format!("build envelope: {e}"))))?;
    let tx_hash = envelope.tx_hash().to_owned();

    // 4. Send via the same txs_client the regular spammer uses.
    let any_envelope = AnyTxEnvelope::Ethereum(envelope);
    let start_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let res = scenario.txs_client.send_tx_envelope(any_envelope).await;
    let error = match res {
        Ok(_) => {
            info!("stream tx[{idx}]: {tx_hash} sent");
            None
        }
        Err(e) => {
            let msg = e
                .as_error_resp()
                .map(|err| err.message.to_string())
                .unwrap_or_else(|| format!("{e}"));
            warn!("stream tx[{idx}]: {tx_hash} failed: {msg}");
            Some(msg)
        }
    };

    // 5. Cache in the tx_actor so its flush loop polls for the receipt.
    scenario
        .tx_actor()
        .cache_run_tx(CacheTx {
            tx_hash,
            start_timestamp_ms: start_ms,
            end_timestamp_ms: None,
            kind: spec.kind.clone(),
            error,
        })
        .await?;

    Ok(())
}

/// Top-level entry point invoked from `main.rs`.
pub async fn spam_stream(db: &SqliteDb, args: SpamStreamCliArgs) -> Result<()> {
    let seed = if let Some(s) = &args.seed {
        RandSeed::seed_from_str(s)
    } else {
        RandSeed::new()
    };

    // Build a decoy TestConfig + agent store so the scenario sets up the
    // requested pool with the requested number of signers.
    let config = build_decoy_config(&args.from_pool);
    let agent_spec = AgentSpec::default()
        .create_accounts(0)
        .setup_accounts(0)
        .spam_accounts(args.pool_size);

    let funder = if let Some(key) = &args.private_key {
        let s = key.trim().trim_start_matches("0x");
        Some(
            alloy::signers::local::PrivateKeySigner::from_slice(
                &alloy::hex::decode(s)
                    .map_err(|e| CliError::Args(ArgsError::Custom(format!("bad priv-key: {e}"))))?,
            )
            .map_err(|e| CliError::Args(ArgsError::Custom(format!("bad priv-key: {e}"))))?,
        )
    } else {
        None
    };
    // TestScenario needs at least one user signer for signer_map. Use a
    // throwaway signer if none was provided (we only sign with pool accounts).
    let user_signers = if let Some(s) = &funder {
        vec![s.clone()]
    } else {
        vec![alloy::signers::local::PrivateKeySigner::random()]
    };

    let cancel = CancellationToken::new();
    let params = TestScenarioParams {
        rpc_url: args.rpc_url.clone(),
        builder_rpc_url: None,
        txs_rpc_url: None,
        signers: user_signers,
        agent_spec,
        tx_type: TxType::Eip1559,
        bundle_type: BundleType::default(),
        pending_tx_timeout: Duration::from_secs(60),
        extra_msg_handles: None,
        sync_nonces_after_batch: false,
        rpc_batch_size: 0,
        gas_price: None,
        scenario_label: Some(format!("stream-{}", args.from_pool)),
        send_raw_tx_sync: false,
        flashblocks_ws_url: None,
    };

    let mut scenario: TestScenario<SqliteDb, RandSeed, TestConfig> = TestScenario::new(
        config,
        Arc::new(db.clone()),
        seed,
        params,
        None,
        (&PROM, &HIST).into(),
        &cancel,
    )
    .await?;

    // Fund the pool from the funder key if provided.
    if !args.skip_funding {
        if let Some(funder) = &funder {
            let addrs = scenario.agent_store.all_signer_addresses();
            if !addrs.is_empty() {
                info!(
                    "funding {} pool account(s) to {} ETH min from {}",
                    addrs.len(),
                    format_ether(args.min_balance),
                    funder.address()
                );
                fund_accounts(
                    &addrs,
                    funder,
                    &scenario.rpc_client,
                    args.min_balance,
                    TxType::Legacy,
                    &Default::default(),
                )
                .await?;
                // Re-sync nonces for the freshly-funded accounts so the first
                // sent tx uses the correct nonce.
                scenario.sync_nonces().await?;
            }
        } else {
            warn!("no funder key supplied; pool accounts must already be funded");
        }
    }

    // Set up tx_actor context so cached txs flush to the DB.
    let start_block = scenario.rpc_client.get_block_number().await?;
    let run_id = 0u64;
    let actor_ctx =
        ActorContext::new(start_block, run_id).with_pending_tx_timeout(Duration::from_secs(60));
    scenario.tx_actor().init_ctx(actor_ctx).await?;

    // Spawn the reader and run the drive loop.
    let (tx, rx) = mpsc::channel::<FunctionCallDefinition>(STREAM_BUFFER);
    let _reader = spawn_stream_reader(&args.from, tx)?;

    let drive_cancel = cancel.clone();
    let sent = tokio::select! {
        res = drive_stream(&mut scenario, rx, run_id, args.from_pool.clone(), args.tps, drive_cancel) => {
            res?
        }
        _ = tokio::signal::ctrl_c() => {
            info!("CTRL-C: stopping stream loop");
            cancel.cancel();
            0
        }
    };

    info!("stream complete: {sent} tx(s) sent; draining pending receipts...");

    tokio::select! {
        _ = scenario.dump_tx_cache(run_id) => {}
        _ = tokio::signal::ctrl_c() => {
            info!("CTRL-C during drain; exiting");
        }
    }
    scenario.shutdown().await;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_spec() {
        let line =
            r#"{"to":"0xdeAD000000000000000000000000000000000000","value":"1","gas_limit":21000}"#;
        let spec: FunctionCallDefinition = serde_json::from_str(line).unwrap();
        assert_eq!(spec.to, "0xdeAD000000000000000000000000000000000000");
        assert_eq!(spec.value.as_deref(), Some("1"));
        assert_eq!(spec.gas_limit, Some(21000));
        assert!(spec.from.is_none() && spec.from_pool.is_none());
    }

    #[test]
    fn parses_spec_with_signature_and_args() {
        let line = r#"{
            "to": "0x4200000000000000000000000000000000000022",
            "signature": "validateMessage(bytes32)",
            "args": ["0x0102030405060708091011121314151617181920212223242526272829303132"],
            "gas_limit": 200000
        }"#;
        let spec: FunctionCallDefinition = serde_json::from_str(line).unwrap();
        assert_eq!(spec.signature.as_deref(), Some("validateMessage(bytes32)"));
        assert_eq!(spec.args.as_ref().unwrap().len(), 1);
        assert_eq!(spec.gas_limit, Some(200000));
    }

    #[tokio::test]
    async fn forward_lines_skips_blank_and_comments_and_emits_specs() {
        let (tx, mut rx) = mpsc::channel::<FunctionCallDefinition>(8);
        let input = b"\n# this is a comment\n{\"to\":\"0xdeAD000000000000000000000000000000000000\",\"value\":\"1\"}\n\n{\"to\":\"0xdeAD000000000000000000000000000000000001\",\"value\":\"2\"}\n";
        let reader = BufReader::new(&input[..]);
        forward_lines(reader, tx).await;
        let mut received = vec![];
        while let Ok(spec) = rx.try_recv() {
            received.push(spec);
        }
        assert_eq!(received.len(), 2);
        assert_eq!(received[0].value.as_deref(), Some("1"));
        assert_eq!(received[1].value.as_deref(), Some("2"));
    }

    #[tokio::test]
    async fn forward_lines_skips_malformed_lines() {
        let (tx, mut rx) = mpsc::channel::<FunctionCallDefinition>(8);
        let input = b"not json at all\n{\"to\":\"0xdeAD000000000000000000000000000000000000\"}\n";
        let reader = BufReader::new(&input[..]);
        forward_lines(reader, tx).await;
        let mut received = vec![];
        while let Ok(spec) = rx.try_recv() {
            received.push(spec);
        }
        assert_eq!(received.len(), 1);
    }
}
