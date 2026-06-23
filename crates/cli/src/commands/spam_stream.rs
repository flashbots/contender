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
    util::{fund_accounts, load_seedfile},
    LATENCY_HIST as HIST, PROM,
};
use alloy::{
    consensus::TxType,
    network::{AnyTxEnvelope, Ethereum, EthereumWallet, NetworkTransactionBuilder},
    primitives::{utils::format_ether, U256},
    providers::Provider,
    signers::local::PrivateKeySigner,
    transports::http::reqwest::Url,
};
use clap::Args;
use contender_core::{
    agent_controller::{AgentClass, AgentStore},
    db::{DbOps, SpamDuration, SpamRunRequest},
    generator::{
        agent_pools::AgentSpec,
        seeder::rand_seed::SeedGenerator,
        templater::Templater,
        util::{complete_tx_request, parse_value},
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
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
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
/// Gas limit used when a stream spec doesn't set one (stream mode doesn't
/// estimate per tx; relay specs always carry an explicit gas_limit).
const DEFAULT_STREAM_GAS_LIMIT: u64 = 200_000;

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
        long_help = "Target transactions per second. This paces how fast specs are pulled \
            off the input stream, NOT how many txs are duplicated: each input spec is sent \
            exactly once. With `0` (the default) specs are sent as fast as they arrive. If \
            the stream supplies fewer specs per second than `--tps`, the rate is bounded by \
            the stream, so a one-line input sends a single tx regardless of the value.",
        default_value_t = 0,
        help_heading = HELP_HEADING_RUNTIME,
    )]
    pub tps: u64,

    /// Minimum balance to keep in each pool account.
    #[arg(
        long,
        long_help = "The minimum balance to keep in each pool account, with units \
            (e.g. \"10 eth\", \"0.5 ether\", \"100 gwei\"). A plain number is parsed as wei.",
        default_value = "0.01 ether",
        value_parser = parse_value,
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

/// Worker (and pool-signer) index for a stream index. Specs are assigned
/// round-robin across the pool, matching `make_strict_call`'s `idx %
/// signers.len()` signer selection, so routing spec `idx` to `worker_index(idx,
/// n)` guarantees each signer is only ever touched by one worker — which is what
/// keeps that signer's nonce handling serial and correct.
fn worker_index(idx: usize, n_workers: usize) -> usize {
    idx % n_workers
}

/// Next local nonce after a send attempt. Advances on an accepted send; on a
/// rejected send the tx never entered the mempool, so the nonce is reused
/// (returned unchanged) to avoid a gap that would stall every later tx from the
/// signer. Correct only because each signer's sends are serial within its worker.
fn next_nonce(nonce: u64, submitted: bool) -> u64 {
    if submitted {
        nonce + 1
    } else {
        nonce
    }
}

/// Drive the stream loop with per-signer concurrency. One worker is spawned per
/// pool signer; each spec is routed (round-robin by idx, matching
/// `make_strict_call`'s signer selection) to the worker that owns its signer, and
/// workers send concurrently. Because each signer's sends stay serial within its
/// worker, nonce assignment and reclaim-on-bounce remain correct with no shared
/// nonce state. Concurrency == pool size; a pool of 1 reproduces serial sending.
async fn drive_stream<S, P>(
    scenario: Arc<TestScenario<SqliteDb, S, P>>,
    mut rx: mpsc::Receiver<FunctionCallDefinition>,
    fallback_pool: String,
    tps: u64,
    cancel: CancellationToken,
) -> Result<(usize, usize)>
where
    S: SeedGenerator + Send + Sync + Clone + 'static,
    P: PlanConfig<String> + Templater<String> + Send + Sync + Clone + 'static,
{
    // Shared, periodically-refreshed gas price: one RPC per interval rather than
    // one per tx (the per-tx fetch was part of the serial throughput ceiling).
    let gas_price = Arc::new(AtomicU64::new(
        scenario.rpc_client.get_gas_price().await? as u64,
    ));
    let mut last_gas_refresh = Instant::now();

    // One worker per pool signer; route so signer k is only ever touched by
    // worker k (matches make_strict_call's `idx % signers.len()` selection), so
    // each signer's nonce + reclaim stay serial and race-free.
    let signers = scenario
        .agent_store
        .get_agent(&fallback_pool)
        .map(|a| a.signers.clone())
        .unwrap_or_default();
    if signers.is_empty() {
        return Err(CliError::Args(ArgsError::Custom(format!(
            "pool '{fallback_pool}' has no signers"
        ))));
    }
    let n_workers = signers.len();

    let mut worker_txs = Vec::with_capacity(n_workers);
    let mut worker_handles = Vec::with_capacity(n_workers);
    let mut worker_backpressured = vec![false; n_workers];
    for signer in signers.into_iter() {
        let (wtx, wrx) = mpsc::channel::<(usize, FunctionCallDefinition)>(64);
        let nonce = scenario.nonces.get(&signer.address()).copied().unwrap_or(0);
        let handle = tokio::spawn(send_worker(
            scenario.clone(),
            wrx,
            signer,
            nonce,
            gas_price.clone(),
        ));
        worker_txs.push(wtx);
        worker_handles.push(handle);
    }

    // Rate limiter: only ticks when tps > 0.
    let mut ticker = if tps > 0 {
        let period = Duration::from_secs_f64(1.0 / tps as f64);
        let mut int = tokio::time::interval(period);
        int.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        Some(int)
    } else {
        None
    };

    let mut idx: usize = 0;
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
                        gas_price.store(gp as u64, Ordering::Relaxed);
                    }
                    last_gas_refresh = Instant::now();
                }

                // Route to the worker that owns this idx's signer.
                let w = worker_index(idx, n_workers);
                match worker_txs[w].try_send((idx, spec)) {
                    Ok(()) => worker_backpressured[w] = false,
                    Err(mpsc::error::TrySendError::Full((idx, spec))) => {
                        if !worker_backpressured[w] {
                            worker_backpressured[w] = true;
                            let capacity = worker_txs[w].max_capacity();
                            StreamEvent::emit(StreamPayload::Backpressure {
                                queued: capacity.saturating_sub(worker_txs[w].capacity()),
                                capacity,
                            });
                        }
                        if worker_txs[w].send((idx, spec)).await.is_err() {
                            break;
                        }
                    }
                    Err(mpsc::error::TrySendError::Closed(_)) => break,
                }
                idx += 1;
            }
        }
    }

    // Close worker inputs and join; sum their tallies.
    drop(worker_txs);
    let mut sent = 0usize;
    let mut failed = 0usize;
    for handle in worker_handles {
        match handle.await {
            Ok((s, f)) => {
                sent += s;
                failed += f;
            }
            Err(e) => warn!("stream: send worker panicked: {e}"),
        }
    }
    Ok((sent, failed))
}

/// Per-signer send worker: pulls `(idx, spec)` for one signer, builds + signs
/// with that signer using a locally-tracked nonce, and sends. Serial within the
/// signer (so the nonce counter and reclaim are race-free); concurrency comes
/// from running one worker per signer. On a send rejection the nonce is reused
/// (not advanced), matching the serial path's reclaim — no gap, no stall.
async fn send_worker<S, P>(
    scenario: Arc<TestScenario<SqliteDb, S, P>>,
    mut rx: mpsc::Receiver<(usize, FunctionCallDefinition)>,
    signer: PrivateKeySigner,
    mut nonce: u64,
    gas_price: Arc<AtomicU64>,
) -> (usize, usize)
where
    S: SeedGenerator + Send + Sync + Clone + 'static,
    P: PlanConfig<String> + Templater<String> + Send + Sync + Clone + 'static,
{
    let wallet = EthereumWallet::from(signer);
    let placeholder_map = HashMap::<String, String>::new();
    let mut sent = 0usize;
    let mut failed = 0usize;

    while let Some((idx, spec)) = rx.recv().await {
        // Stream mode only builds EIP-1559 txs; reject blob/7702 specs.
        if spec.blob_data.is_some() || spec.authorization_address.is_some() {
            warn!("stream tx[{idx}]: blob/7702 specs are unsupported in stream mode; skipping");
            failed += 1;
            continue;
        }

        let strict = match scenario.make_strict_call(&spec, idx) {
            Ok(s) => s,
            Err(e) => {
                warn!("stream tx[{idx}]: build failed: {e}");
                failed += 1;
                continue;
            }
        };
        let tx_req = match scenario
            .get_templater()
            .template_function_call(&strict, &placeholder_map)
        {
            Ok(t) => t,
            Err(e) => {
                warn!("stream tx[{idx}]: template failed: {e}");
                failed += 1;
                continue;
            }
        };

        // Build + sign with a worker-local nonce (no &mut scenario, no shared
        // nonce map), using the spec's gas limit instead of a per-tx estimate.
        let gp = gas_price.load(Ordering::Relaxed) as u128;
        let gas_limit = tx_req.gas.unwrap_or(DEFAULT_STREAM_GAS_LIMIT);
        let priority_fee = tx_req.max_priority_fee_per_gas.unwrap_or(gp / 10);
        let mut full_tx = tx_req;
        full_tx.nonce = Some(nonce);
        complete_tx_request(
            &mut full_tx,
            scenario.tx_type,
            gp,
            priority_fee,
            gas_limit,
            scenario.chain_id,
            0,
        );
        let envelope = match <alloy::rpc::types::TransactionRequest as NetworkTransactionBuilder<
            Ethereum,
        >>::build(full_tx, &wallet)
        .await
        {
            Ok(e) => e,
            Err(e) => {
                warn!("stream tx[{idx}]: build envelope failed: {e}");
                failed += 1;
                continue;
            }
        };
        let tx_hash = envelope.tx_hash().to_owned();
        let start_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let res = scenario
            .txs_client
            .send_tx_envelope(AnyTxEnvelope::Ethereum(envelope))
            .await;
        let submitted = res.is_ok();
        let error = match res {
            Ok(_) => {
                info!("stream tx[{idx}]: {tx_hash} sent");
                sent += 1;
                None
            }
            Err(e) => {
                let msg = e
                    .as_error_resp()
                    .map(|err| err.message.to_string())
                    .unwrap_or_else(|| format!("{e}"));
                warn!("stream tx[{idx}]: {tx_hash} failed: {msg}");
                failed += 1;
                Some(msg)
            }
        };
        // Advance the nonce only on an accepted send; a rejected send never
        // entered the mempool, so reuse the nonce (no gap that would stall the
        // signer behind it).
        nonce = next_nonce(nonce, submitted);
        StreamEvent::emit(StreamPayload::TxResult {
            idx,
            tx_hash: tx_hash.to_string(),
            start_timestamp_ms: start_ms,
            kind: spec.kind.clone(),
            error: error.clone(),
        });
        let _ = scenario
            .tx_actor()
            .cache_run_tx(CacheTx {
                tx_hash,
                start_timestamp_ms: start_ms,
                end_timestamp_ms: None,
                kind: spec.kind.clone(),
                error,
            })
            .await;
    }

    (sent, failed)
}

/// Top-level entry point invoked from `main.rs`.
pub async fn spam_stream(db: &SqliteDb, args: SpamStreamCliArgs, data_dir: &Path) -> Result<()> {
    // Fall back to the persisted seedfile (not a fresh random seed) when --seed
    // is unset, matching `spam`/`setup`/`campaign`. This keeps the executor
    // pool's addresses stable across invocations, so a pool funded in one run
    // is still funded for a later `--skip-funding` run.
    let seed = RandSeed::seed_from_str(&match &args.seed {
        Some(s) => s.clone(),
        None => load_seedfile(data_dir)?,
    });

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
        // Must be true: gates TestScenario::sync_nonces(). With it false the
        // explicit sync_nonces() calls below are silent no-ops, leaving the
        // pool accounts' nonces unset, so prepare_tx_request fails every send
        // with NonceMissing ("core error"). Stream mode sends one tx at a time
        // and never hits the post-batch sync path, so enabling this only makes
        // the initial pool-nonce sync actually run.
        sync_nonces_after_batch: true,
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

    // Share the now-fully-initialized scenario across the per-signer send
    // workers. All &mut setup (nonce sync, funding, actor ctx) is already done.
    let scenario = Arc::new(scenario);
    let (sent, failed) = drive_stream(
        scenario.clone(),
        rx,
        args.from_pool.clone(),
        args.tps,
        cancel.clone(),
    )
    .await?;
    StreamEvent::emit(StreamPayload::Summary { sent, failed });

    info!("stream complete: {sent} sent, {failed} failed; draining pending receipts...");

    // drive_stream has joined all workers, so their Arc clones are dropped and we
    // can reclaim exclusive ownership for the receipt drain + shutdown.
    let mut scenario = Arc::try_unwrap(scenario).map_err(|_| {
        CliError::Args(ArgsError::Custom(
            "scenario still shared after drive_stream returned".into(),
        ))
    })?;

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
    use clap::{Command, FromArgMatches};

    /// `SpamStreamCliArgs` derives `Args` (not `Parser`), so build a throwaway
    /// `Command` from it to introspect the argument configuration in tests.
    fn args_command() -> Command {
        SpamStreamCliArgs::augment_args(Command::new("spam-stream"))
    }

    #[test]
    fn args_config_is_valid() {
        // Catches clap misconfiguration (conflicting attrs, bad value_parser, etc.).
        args_command().debug_assert();
    }

    #[test]
    fn tps_flag_has_long_help() {
        // The review asked for long_help clarifying that --tps paces the stream
        // rather than duplicating txs. Assert it's wired up and mentions the
        // single-spec behavior so the explanation can't silently regress.
        let cmd = args_command();
        let tps = cmd
            .get_arguments()
            .find(|a| a.get_id() == "tps")
            .expect("tps arg exists");
        let long_help = tps.get_long_help().expect("tps has long_help").to_string();
        assert!(long_help.contains("once"), "long_help: {long_help}");
        assert!(long_help.contains("stream"), "long_help: {long_help}");
    }

    /// Parse CLI args from a token list, the way clap would at runtime, so we
    /// can assert how value strings are coerced into `SpamStreamCliArgs`.
    fn parse_args(tokens: &[&str]) -> SpamStreamCliArgs {
        let matches = args_command()
            .get_matches_from(std::iter::once("spam-stream").chain(tokens.iter().copied()));
        SpamStreamCliArgs::from_arg_matches(&matches).expect("from_arg_matches")
    }

    #[test]
    fn min_balance_accepts_unit_strings() {
        // The review asked --min-balance to accept unit-value strings via parse_value.
        // 10 eth == 10e18 wei.
        let eth = parse_args(&["--min-balance", "10 eth"]);
        assert_eq!(eth.min_balance, U256::from(10_000_000_000_000_000_000u128));
        // A plain number is still wei (parse_value's fallback).
        let wei = parse_args(&["--min-balance", "12345"]);
        assert_eq!(wei.min_balance, U256::from(12345u64));
    }

    #[test]
    fn min_balance_default_is_point_zero_one_ether() {
        // Default must round-trip through value_parser to 0.01 ETH = 1e16 wei.
        let args = parse_args(&[]);
        assert_eq!(args.min_balance, U256::from(10_000_000_000_000_000u128));
    }

    #[test]
    fn pool_addresses_are_deterministic_for_a_seed() {
        // The --skip-funding bug was that a fresh random seed each run produced
        // different pool addresses, so a pool funded in run 1 looked unfunded in
        // run 2. Pin the invariant the fix relies on: a given seed always yields
        // the same pool addresses, and different seeds yield different ones.
        let seed_a = RandSeed::seed_from_str("0xabc");
        let s1 = build_pool_agent_store("executors", 5, &seed_a);
        let s2 = build_pool_agent_store("executors", 5, &seed_a);
        assert_eq!(s1.all_signer_addresses(), s2.all_signer_addresses());
        assert_eq!(s1.all_signer_addresses().len(), 5);

        let seed_b = RandSeed::seed_from_str("0xdef");
        let s3 = build_pool_agent_store("executors", 5, &seed_b);
        assert_ne!(s1.all_signer_addresses(), s3.all_signer_addresses());
    }

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

    #[test]
    fn worker_index_is_round_robin_and_per_signer_stable() {
        // The per-signer concurrency model is correct only if each signer is
        // touched by exactly one worker. With n workers: one full cycle hits
        // every worker once, idx and idx+n return to the same worker (so the
        // same signer), and adjacent specs spread across workers.
        let n = 4;
        let mut cycle: Vec<usize> = (0..n).map(|i| worker_index(i, n)).collect();
        cycle.sort_unstable();
        cycle.dedup();
        assert_eq!(
            cycle,
            vec![0, 1, 2, 3],
            "a full cycle hits each worker once"
        );
        for idx in 0..10 {
            assert_eq!(worker_index(idx, n), worker_index(idx + n, n));
        }
        assert_ne!(worker_index(0, n), worker_index(1, n));
    }

    #[test]
    fn worker_routing_matches_pool_signer_selection() {
        // Routing must pick the same index make_strict_call uses for the signer
        // (idx % signers.len()), so worker k always owns pool signer k.
        let seed = RandSeed::seed_from_str("0xabc");
        let store = build_pool_agent_store("executors", 5, &seed);
        let agent = store.get_agent("executors").expect("pool exists");
        let n = agent.signers.len();
        assert_eq!(n, 5);
        for idx in 0..23usize {
            assert_eq!(worker_index(idx, n), idx % agent.signers.len());
        }
    }

    #[test]
    fn next_nonce_reuses_on_rejection() {
        // Accepted send advances the nonce; rejected send reuses it (no gap).
        assert_eq!(next_nonce(7, true), 8);
        assert_eq!(next_nonce(7, false), 7);
    }

    #[test]
    fn pool_signers_are_distinct() {
        // Per-signer workers require the pool to have distinct signers.
        let seed = RandSeed::seed_from_str("0xfeed");
        let store = build_pool_agent_store("executors", 8, &seed);
        let mut addrs = store.all_signer_addresses();
        assert_eq!(addrs.len(), 8);
        addrs.sort_unstable();
        addrs.dedup();
        assert_eq!(addrs.len(), 8, "pool signers must be distinct");
    }
}
