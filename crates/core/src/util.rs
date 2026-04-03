use crate::{db::DbOps, generator::types::AnyProvider, Result};
use alloy::{consensus::TxType, providers::Provider, signers::local::PrivateKeySigner};
use serde::Serialize;
use std::{str::FromStr, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};
use tracing_subscriber::EnvFilter;

/// Derive the block time from the first two blocks after genesis.
pub async fn get_block_time(rpc_client: &AnyProvider) -> Result<u64> {
    // derive block time from first two blocks after genesis.
    // if >2 blocks don't exist, assume block time is 1s
    let block_num = rpc_client.get_block_number().await?;
    let block_time_secs = if block_num >= 2 {
        let mut timestamps = vec![];
        for i in [1_u64, 2] {
            debug!("getting timestamp for block {i}");
            let block = rpc_client.get_block_by_number(i.into()).await?;
            if let Some(block) = block {
                timestamps.push(block.header.timestamp);
            }
        }
        if timestamps.len() == 2 {
            (timestamps[1] - timestamps[0]).max(1)
        } else {
            1
        }
    } else {
        1
    };
    Ok(block_time_secs)
}

/// Returns blob fee for EIP-4844 transactions, or 0 for all other tx types.
/// Skips the RPC call entirely when blobs aren't needed.
pub async fn get_blob_fee_maybe(rpc_client: &AnyProvider, tx_type: TxType) -> u128 {
    if tx_type != TxType::Eip4844 {
        return 0;
    }
    let res = rpc_client.get_blob_base_fee().await;
    if res.is_err() {
        debug!("failed to get blob base fee; defaulting to 0");
    }
    res.unwrap_or(0)
}

#[derive(Debug, Clone)]
pub struct ExtraTxParams {
    pub gas_price: u128,
    pub blob_gas_price: u128,
    pub chain_id: u64,
}

impl From<(u128, u128, u64)> for ExtraTxParams {
    fn from((gas_price, blob_gas_price, chain_id): (u128, u128, u64)) -> Self {
        ExtraTxParams {
            gas_price,
            blob_gas_price,
            chain_id,
        }
    }
}

pub const DEFAULT_PRV_KEYS: [&str; 10] = [
    "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
    "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d",
    "0x5de4111afa1a4b94908f83103eb1f1706367c2e68ca870fc3fb9a804cdab365a",
    "0x7c852118294e51e653712a81e05800f419141751be58f605c371e15141b007a6",
    "0x47e179ec197488593b187f80a00eb0da91f1b9d0b13f8733639f19c30a34926a",
    "0x8b3a350cf5c34c9194ca85829a2df0ec3153be0318b5e2d3348e872092edffba",
    "0x92db14e403b83dfe3df233f83dfa3a0d7096f21ca9b0d6d6b8d88b2b4ec1564e",
    "0x4bbbf85ce3377467afe5d46f804f221813b2bb87f24d81f60f1fcdbf7cbf4356",
    "0xdbda1821b80551c9d65939329250298aa3472ba22feea921c0cf5d620ea67b97",
    "0x2a871d0798f97d79848a013d4936a73bf4cc922c825d33c1cf7073dff6d409c6",
];

pub fn default_signers() -> Vec<PrivateKeySigner> {
    DEFAULT_PRV_KEYS
        .into_iter()
        .map(|k| PrivateKeySigner::from_str(k).expect("Invalid private key"))
        .collect::<Vec<PrivateKeySigner>>()
}

#[derive(Debug)]
pub struct TracingOptions {
    pub ansi: bool,
    pub target: bool,
    pub line_number: bool,
}

impl Default for TracingOptions {
    fn default() -> Self {
        Self {
            ansi: true,
            target: false,
            line_number: false,
        }
    }
}

impl TracingOptions {
    pub fn with_ansi(mut self, ansi: bool) -> Self {
        self.ansi = ansi;
        self
    }

    pub fn with_target(mut self, target: bool) -> Self {
        self.target = target;
        self
    }

    pub fn with_line_number(mut self, line_number: bool) -> Self {
        self.line_number = line_number;
        self
    }
}

pub fn init_core_tracing(filter: Option<EnvFilter>, opts: TracingOptions) {
    let filter = filter.unwrap_or(EnvFilter::new("info"));

    tracing_subscriber::fmt()
        .with_ansi(opts.ansi)
        .with_env_filter(filter)
        .with_target(opts.target)
        .with_line_number(opts.line_number)
        .init();
}

/// Structured JSON report emitted periodically during spam runs.
#[derive(Debug, Clone, Serialize)]
pub struct SpamProgressReport {
    pub elapsed_s: u64,
    pub txs_sent: u64,
    pub txs_confirmed: u64,
    pub txs_failed: u64,
    pub current_tps: f64,
}

/// Prints incremental progress report for a spam run. Returns None if call to db's `get_run_txs` fails.
pub fn print_progress_report<D: DbOps + Send + Sync + 'static>(
    db: &D,
    run_id: u64,
    start: std::time::Instant,
    planned_tx_count: Option<u64>,
) -> Option<()> {
    let elapsed = start.elapsed();
    let elapsed_s = elapsed.as_secs();

    let Ok((txs_confirmed, txs_failed)) = db.get_run_txs(run_id).map(|txs| {
        let confirmed = txs
            .iter()
            .filter(|tx| tx.block_number.is_some() && tx.error.is_none())
            .count() as u64;
        let failed = txs.iter().filter(|tx| tx.error.is_some()).count() as u64;
        (confirmed, failed)
    }) else {
        return None;
    };

    // txs_sent is the planned count capped by elapsed time,
    // or confirmed + failed if we have more data than planned
    let txs_sent = (txs_confirmed + txs_failed).max(planned_tx_count.unwrap_or(0).min(
        // rough estimate based on elapsed time
        txs_confirmed + txs_failed,
    ));

    let current_tps = if elapsed_s > 0 {
        txs_confirmed as f64 / elapsed_s as f64
    } else {
        0.0
    };

    let report = SpamProgressReport {
        elapsed_s,
        txs_sent,
        txs_confirmed,
        txs_failed,
        current_tps: (current_tps * 10.0).round() / 10.0,
    };

    // tracing span annotates the log for easy identification later
    let span = tracing::info_span!("spam_progress", run_id = run_id);
    if let Ok(json) = serde_json::to_string(&report) {
        let _enter = span.enter();
        info!("{json}");
    }

    Some(())
}

/// Spawns a background task that periodically queries the DB and prints
/// a structured JSON progress report to stdout.
/// Returns a cancellation token that should be cancelled when spam is done.
pub fn spawn_spam_report_task<D: DbOps + Clone + Send + Sync + 'static>(
    db: &D,
    run_id: u64,
    interval_secs: u64,
    planned_tx_count: u64,
) -> CancellationToken {
    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();
    let db = Arc::new(db.clone());

    // Capture the current session ID (if set) so the spawned task inherits it.
    // This allows the server's SessionLogRouter layer to route these logs
    // to the correct per-session broadcast channel.
    let session_id = crate::CURRENT_SESSION_ID.try_with(|id| *id).ok();

    let future = async move {
        let start = std::time::Instant::now();
        let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
        // Skip the first immediate tick
        interval.tick().await;

        loop {
            tokio::select! {
                _ = cancel_clone.cancelled() => break,
                _ = interval.tick() => {
                    if print_progress_report(db.as_ref(), run_id, start, Some(planned_tx_count)).is_none() {
                        continue;
                    }
                }
            }
        }
    };

    if let Some(id) = session_id {
        tokio::task::spawn(crate::CURRENT_SESSION_ID.scope(id, future));
    } else {
        tokio::task::spawn(future);
    }

    cancel
}
