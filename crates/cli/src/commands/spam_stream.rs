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
    agent_controller::{AgentClass, AgentStore},
    db::{DbOps, SpamDuration, SpamRunRequest},
    generator::{
        agent_pools::AgentSpec, seeder::rand_seed::SeedGenerator, templater::Templater,
        FunctionCallDefinition, Generator, PlanConfig, RandSeed,
    },
    spammer::tx_actor::{ActorContext, CacheTx},
    test_scenario::{TestScenario, TestScenarioParams},
    BundleType,
};
use contender_sqlite::SqliteDb;
use contender_testfile::TestConfig;
use serde::Serialize;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, BufReader},
    sync::mpsc,
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

/// Default agent-pool name when the user does not specify one.
const DEFAULT_POOL: &str = "executors";
/// Channel buffer between the reader task and the spam loop.
const STREAM_BUFFER: usize = 256;
/// Schema version of the structured stdout output. Bump when the envelope
/// shape changes in a backward-incompatible way.
const OUTPUT_VERSION: u32 = 1;
/// How often to refresh the cached gas price during the stream loop. Avoids an
/// RPC round-trip on every single tx.
const GAS_REFRESH_INTERVAL: Duration = Duration::from_secs(6);

/// Versioned, tagged envelope written to stdout (one JSON line per event) so
/// downstream consumers can evolve with the schema. The `version` field pins
/// the schema and the `type` tag (via `payload`) discriminates event kinds.
#[derive(Debug, Serialize)]
struct StreamEvent {
    version: u32,
    #[serde(flatten)]
    payload: StreamPayload,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum StreamPayload {
    /// Emitted once per input spec after the send attempt.
    TxResult {
        /// Zero-based index of the spec in the stream.
        idx: usize,
        /// Transaction hash (present even when the send RPC call failed).
        tx_hash: String,
        /// Unix-epoch milliseconds when the send was attempted.
        start_timestamp_ms: u128,
        /// Optional `kind` carried over from the input spec.
        #[serde(skip_serializing_if = "Option::is_none")]
        kind: Option<String>,
        /// Send-time error from the RPC, if any.
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// Buffer saturated; the reader is blocking. Signals the producer to slow down.
    Backpressure { queued: usize, capacity: usize },
    /// Emitted once when the stream finishes (EOF or cancellation).
    Summary { sent: usize, failed: usize },
}

impl StreamEvent {
    fn emit(payload: StreamPayload) {
        let event = StreamEvent {
            version: OUTPUT_VERSION,
            payload,
        };
        // Serialization of these fixed-shape payloads cannot fail.
        if let Ok(line) = serde_json::to_string(&event) {
            println!("{line}");
        }
    }
}

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
    // Validate file existence eagerly so a bad path is a clean CLI error rather
    // than a silent warning from inside the spawned task.
    if from != "stdin" && !PathBuf::from(from).exists() {
        return Err(CliError::Args(ArgsError::Custom(format!(
            "stream source file not found: {from}"
        ))));
    }

    let from = from.to_owned();
    Ok(tokio::spawn(async move {
        let reader: Box<dyn AsyncBufRead + Unpin + Send> = if from == "stdin" {
            Box::new(BufReader::new(tokio::io::stdin()))
        } else {
            match tokio::fs::File::open(&from).await {
                Ok(f) => Box::new(BufReader::new(f)),
                Err(e) => {
                    warn!("failed to open stream source {from}: {e}");
                    return;
                }
            }
        };
        forward_lines(reader, tx).await;
    }))
}

async fn forward_lines<R>(reader: R, tx: mpsc::Sender<FunctionCallDefinition>)
where
    R: tokio::io::AsyncBufRead + Unpin,
{
    let mut lines = reader.lines();
    let mut line_no: u64 = 0;
    // Emit the backpressure event once per saturation episode, not per blocked send.
    let mut backpressured = false;
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                line_no += 1;
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    continue;
                }
                match serde_json::from_str::<FunctionCallDefinition>(trimmed) {
                    Ok(spec) => match tx.try_send(spec) {
                        Ok(()) => backpressured = false,
                        Err(mpsc::error::TrySendError::Full(spec)) => {
                            if !backpressured {
                                backpressured = true;
                                let capacity = tx.max_capacity();
                                StreamEvent::emit(StreamPayload::Backpressure {
                                    queued: capacity.saturating_sub(tx.capacity()),
                                    capacity,
                                });
                            }
                            // Block until a slot frees, applying real backpressure.
                            if tx.send(spec).await.is_err() {
                                return; // receiver dropped
                            }
                        }
                        Err(mpsc::error::TrySendError::Closed(_)) => return,
                    },
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

/// Build an `AgentStore` holding a single spam pool named `from_pool` with
/// `pool_size` signers, derived from `seed`. Stream mode provisions the pool
/// directly instead of round-tripping through a scenario `TestConfig`, since
/// it never executes any pre-defined spam steps.
fn build_pool_agent_store(from_pool: &str, pool_size: usize, seed: &RandSeed) -> AgentStore {
    let mut store = AgentStore::new();
    if pool_size > 0 {
        store.add_new_agent(from_pool, pool_size, seed, AgentClass::Spammer);
    }
    store
}

/// Drive the stream loop: pull specs, build/sign/send txs, cache in the
/// tx_actor for receipt tracking. Returns when the stream channel closes.
async fn drive_stream<S, P>(
    scenario: &mut TestScenario<SqliteDb, S, P>,
    mut rx: mpsc::Receiver<FunctionCallDefinition>,
    fallback_pool: String,
    tps: u64,
    cancel: CancellationToken,
) -> Result<(usize, usize)>
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

    // Cache the gas price instead of fetching it per tx; refreshed below.
    let mut gas_price = scenario.rpc_client.get_gas_price().await?;
    let mut last_gas_refresh = Instant::now();

    let mut sent: usize = 0;
    let mut failed: usize = 0;
    let mut idx: usize = 0;
    let placeholder_map = HashMap::<String, String>::new();

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

                if last_gas_refresh.elapsed() >= GAS_REFRESH_INTERVAL {
                    if let Ok(gp) = scenario.rpc_client.get_gas_price().await {
                        gas_price = gp;
                    }
                    last_gas_refresh = Instant::now();
                }

                match send_one(scenario, &spec, idx, &placeholder_map, gas_price).await {
                    Ok(true) => sent += 1,
                    Ok(false) => failed += 1,
                    Err(e) => {
                        failed += 1;
                        warn!("stream: failed to send tx (idx {idx}): {e}");
                    }
                }
                idx += 1;
            }
        }
    }

    Ok((sent, failed))
}

/// Build a single transaction from a stream spec, sign it, send it, and cache
/// it in the tx_actor for receipt tracking.
async fn send_one<S, P>(
    scenario: &mut TestScenario<SqliteDb, S, P>,
    spec: &FunctionCallDefinition,
    idx: usize,
    placeholder_map: &HashMap<String, String>,
    gas_price: u128,
) -> Result<bool>
where
    S: SeedGenerator + Send + Sync + Clone,
    P: PlanConfig<String> + Templater<String> + Send + Sync + Clone,
{
    // Stream mode only builds EIP-1559 txs (no blob gas price, no auth list), so
    // reject blob (4844) / setCode (7702) specs up front instead of silently
    // producing an invalid tx.
    if spec.blob_data.is_some() || spec.authorization_address.is_some() {
        warn!("stream tx[{idx}]: blob/7702 specs are unsupported in stream mode; skipping");
        return Ok(false);
    }

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

    // 3. Assign nonce/gas-limit + sign using the cached gas price.
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

    let submitted = error.is_none();

    // 5. Emit a structured result line to stdout (mirrors the input stream so
    //    reactive callers can correlate sends with their specs).
    StreamEvent::emit(StreamPayload::TxResult {
        idx,
        tx_hash: tx_hash.to_string(),
        start_timestamp_ms: start_ms,
        kind: spec.kind.clone(),
        error: error.clone(),
    });

    // 6. Cache in the tx_actor so its flush loop polls for the receipt.
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

    Ok(submitted)
}

/// Top-level entry point invoked from `main.rs`.
pub async fn spam_stream(db: &SqliteDb, args: SpamStreamCliArgs) -> Result<()> {
    let seed = if let Some(s) = &args.seed {
        RandSeed::seed_from_str(s)
    } else {
        RandSeed::new()
    };

    // Provision the executor pool directly. Stream mode never runs pre-defined
    // create/setup/spam steps, so the scenario starts from an empty config and
    // we inject the pool's signers below.
    let config = TestConfig::new();
    let pool_store = build_pool_agent_store(&args.from_pool, args.pool_size, &seed);
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

    // Register the pool's signers with the scenario so it can sign with them
    // and track their nonces, then sync nonces from the RPC.
    for (_name, store) in pool_store.all_agents() {
        for signer in &store.signers {
            scenario.signer_map.insert(signer.address(), signer.clone());
        }
    }
    scenario.agent_store = pool_store;
    scenario.sync_nonces().await?;

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

    // Register a run row so receipts dump under a real run_id; run_txs has a FK
    // into runs, so a bogus id (e.g. 0) would orphan them.
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as usize;
    let run = SpamRunRequest {
        timestamp,
        tx_count: 0, // unbounded stream; the real count lands as txs flush
        scenario_name: format!("stream-{}", args.from_pool),
        campaign_id: None,
        campaign_name: None,
        stage_name: None,
        rpc_url: args.rpc_url.to_string(),
        txs_per_duration: args.tps,
        duration: SpamDuration::Seconds(0),
        pending_timeout: Duration::from_secs(60),
    };
    let run_id = db
        .insert_run(&run)
        .map_err(|e| contender_core::Error::Db(e.into()))?;
    info!(run_id, "created stream run");

    // Set up tx_actor context so cached txs flush to the DB.
    let start_block = scenario.rpc_client.get_block_number().await?;
    let actor_ctx =
        ActorContext::new(start_block, run_id).with_pending_tx_timeout(Duration::from_secs(60));
    scenario.tx_actor().init_ctx(actor_ctx).await?;

    // Spawn the reader and run the drive loop.
    let (tx, rx) = mpsc::channel::<FunctionCallDefinition>(STREAM_BUFFER);
    let _reader = spawn_stream_reader(&args.from, tx)?;

    // CTRL-C cancels the loop; drive_stream observes the token and returns its counts.
    let ctrlc_cancel = cancel.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        ctrlc_cancel.cancel();
    });

    let (sent, failed) = drive_stream(
        &mut scenario,
        rx,
        args.from_pool.clone(),
        args.tps,
        cancel.clone(),
    )
    .await?;
    StreamEvent::emit(StreamPayload::Summary { sent, failed });

    info!("stream complete: {sent} sent, {failed} failed; draining pending receipts...");

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
    fn tx_result_envelope_is_versioned_and_tagged() {
        let event = StreamEvent {
            version: OUTPUT_VERSION,
            payload: StreamPayload::TxResult {
                idx: 3,
                tx_hash: "0xabc".to_string(),
                start_timestamp_ms: 1_733_155_200_000,
                kind: Some("validate".to_string()),
                error: None,
            },
        };
        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&event).unwrap()).unwrap();
        assert_eq!(json["version"], 1);
        assert_eq!(json["type"], "tx_result");
        assert_eq!(json["idx"], 3);
        assert_eq!(json["tx_hash"], "0xabc");
        assert_eq!(json["kind"], "validate");
        // `error` is omitted when None.
        assert!(json.get("error").is_none());
    }

    #[test]
    fn summary_and_backpressure_envelopes_are_tagged() {
        let summary = serde_json::to_value(StreamEvent {
            version: OUTPUT_VERSION,
            payload: StreamPayload::Summary { sent: 9, failed: 1 },
        })
        .unwrap();
        assert_eq!(summary["version"], 1);
        assert_eq!(summary["type"], "summary");
        assert_eq!(summary["sent"], 9);
        assert_eq!(summary["failed"], 1);

        let backpressure = serde_json::to_value(StreamEvent {
            version: OUTPUT_VERSION,
            payload: StreamPayload::Backpressure {
                queued: 256,
                capacity: 256,
            },
        })
        .unwrap();
        assert_eq!(backpressure["type"], "backpressure");
        assert_eq!(backpressure["queued"], 256);
        assert_eq!(backpressure["capacity"], 256);
    }

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
