use alloy::{
    eips::BlockNumberOrTag,
    network::AnyNetwork,
    primitives::{Address, Bytes, B256, U256},
    providers::{DynProvider, Provider, ProviderBuilder},
    rpc::client::ClientBuilder,
};
use clap::{Args, Subcommand};
use contender_core::{
    buckets::Bucket,
    db::{DbOps, SpamDuration, SpamRunRequest},
    provider::LoggingLayer,
    test_scenario::Url,
};
use contender_report::{
    chart::rpc_latency::LatencyChart, command::RpcLatencyQuantiles, gen_html::RpcReportMetadata,
};
use contender_sqlite::SqliteDb;
use prometheus::core::Collector;
use serde::Serialize;
use std::{collections::BTreeMap, path::Path, time::Duration};
use tokio::{task::JoinSet, time::MissedTickBehavior};
use tracing::{info, warn};

use crate::{error::CliError, LATENCY_HIST, PROM};

#[derive(Clone, Debug, Args)]
pub struct RpcCliArgs {
    /// RPC endpoint URL.
    #[arg(
        env = "RPC_URL",
        short = 'r',
        long = "rpc-url",
        default_value = "http://localhost:8545"
    )]
    pub rpc_url: Url,

    /// Requests per second.
    #[arg(long, default_value_t = 1)]
    pub rps: u64,

    /// Duration in seconds.
    #[arg(short, long, default_value_t = 1)]
    pub duration: u64,

    /// Generate an HTML report after the spam run.
    #[arg(long)]
    pub gen_report: bool,

    #[command(subcommand)]
    pub method: RpcMethodSubcommand,
}

// Transaction call object used by eth_call, eth_estimateGas, eth_sendTransaction.
#[derive(Debug, Clone, Args, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TxCallObject {
    /// From address.
    pub from: Address,
    /// To address.
    pub to: Address,
    /// Input data (hex).
    pub input: Bytes,
    /// Gas limit.
    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gas: Option<U256>,
    /// Gas price.
    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gas_price: Option<U256>,
    /// Value in wei.
    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<U256>,
    /// Nonce.
    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<u64>,
}

// Filter object for eth_getLogs.
#[derive(Debug, Clone, Args, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogFilterObject {
    /// Start block (number or tag).
    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_block: Option<BlockNumberOrTag>,
    /// End block (number or tag).
    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_block: Option<BlockNumberOrTag>,
    /// Contract address to filter.
    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<Address>,
    /// Topic filters (up to 4).
    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topics: Option<Vec<B256>>,
    /// Block hash (alternative to fromBlock/toBlock).
    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_hash: Option<B256>,
}

fn push_param(out: &mut Vec<serde_json::Value>, val: &impl Serialize) {
    out.push(serde_json::to_value(val).expect("infallible serialization"));
}

fn push_optional(out: &mut Vec<serde_json::Value>, val: &Option<impl Serialize>) {
    if let Some(inner) = val {
        push_param(out, inner);
    }
}

fn tx_object_params(tx: &TxCallObject, block: &Option<BlockNumberOrTag>) -> Vec<serde_json::Value> {
    let mut out = Vec::with_capacity(2);
    push_param(&mut out, tx);
    push_optional(&mut out, block);
    out
}

#[derive(Debug, Clone, Args)]
pub struct EthGetBalanceArgs {
    /// Account address.
    pub address: Address,
    /// Block number or tag.
    #[arg(long, default_value = "latest")]
    pub block: Option<BlockNumberOrTag>,
}

#[derive(Debug, Clone, Args)]
pub struct EthGetTransactionCountArgs {
    /// Account address.
    pub address: Address,
    /// Block number or tag.
    #[arg(long, default_value = "latest")]
    pub block: Option<BlockNumberOrTag>,
}

#[derive(Debug, Clone, Args)]
pub struct EthGetCodeArgs {
    /// Account address.
    pub address: Address,
    /// Block number or tag.
    #[arg(long, default_value = "latest")]
    pub block: Option<BlockNumberOrTag>,
}

#[derive(Debug, Clone, Args)]
pub struct EthGetStorageAtArgs {
    /// Account address.
    pub address: Address,
    /// Storage position.
    pub position: U256,
    /// Block number or tag.
    #[arg(long, default_value = "latest")]
    pub block: Option<BlockNumberOrTag>,
}

#[derive(Debug, Clone, Args)]
pub struct EthGetBlockByNumberArgs {
    /// Block number or tag.
    pub block: BlockNumberOrTag,
    /// Return full transaction objects.
    #[arg(long, default_value_t = false)]
    pub full_txs: bool,
}

#[derive(Debug, Clone, Args)]
pub struct EthGetBlockByHashArgs {
    /// Block hash.
    pub hash: B256,
    /// Return full transaction objects.
    #[arg(long, default_value_t = false)]
    pub full_txs: bool,
}

#[derive(Debug, Clone, Args)]
pub struct EthGetTransactionByHashArgs {
    /// Transaction hash.
    pub hash: B256,
}

#[derive(Debug, Clone, Args)]
pub struct EthGetTransactionReceiptArgs {
    /// Transaction hash.
    pub hash: B256,
}

#[derive(Debug, Clone, Args)]
pub struct EthGetBlockTransactionCountByNumberArgs {
    /// Block number or tag.
    pub block: BlockNumberOrTag,
}

#[derive(Debug, Clone, Args)]
pub struct EthGetBlockTransactionCountByHashArgs {
    /// Block hash.
    pub hash: B256,
}

#[derive(Debug, Clone, Args)]
pub struct EthSendRawTransactionArgs {
    /// Signed transaction data (hex).
    pub data: Bytes,
}

#[derive(Debug, Clone, Args)]
pub struct Web3Sha3Args {
    /// Data to hash with Keccak-256.
    pub data: Bytes,
}

#[derive(Debug, Clone, Args)]
pub struct EthSignArgs {
    /// Account address.
    pub address: Address,
    /// Message to sign (hex).
    pub message: Bytes,
}

#[derive(Debug, Clone, Args)]
pub struct EthSignTransactionArgs {
    #[command(flatten)]
    pub tx: TxCallObject,
}

#[derive(Debug, Clone, Args)]
pub struct EthGetTransactionByBlockHashAndIndexArgs {
    /// Block hash.
    pub block_hash: B256,
    /// Transaction index position.
    pub index: U256,
}

#[derive(Debug, Clone, Args)]
pub struct EthGetTransactionByBlockNumberAndIndexArgs {
    /// Block number or tag.
    pub block: BlockNumberOrTag,
    /// Transaction index position.
    pub index: U256,
}

#[derive(Debug, Clone, Args)]
pub struct EthGetUncleCountByBlockHashArgs {
    /// Block hash.
    pub hash: B256,
}

#[derive(Debug, Clone, Args)]
pub struct EthGetUncleCountByBlockNumberArgs {
    /// Block number or tag.
    pub block: BlockNumberOrTag,
}

#[derive(Debug, Clone, Args)]
pub struct EthGetUncleByBlockHashAndIndexArgs {
    /// Block hash.
    pub block_hash: B256,
    /// Uncle index position.
    pub index: U256,
}

#[derive(Debug, Clone, Args)]
pub struct EthGetUncleByBlockNumberAndIndexArgs {
    /// Block number or tag.
    pub block: BlockNumberOrTag,
    /// Uncle index position.
    pub index: U256,
}

#[derive(Debug, Clone, Args)]
pub struct EthUninstallFilterArgs {
    /// Filter ID.
    pub filter_id: U256,
}

#[derive(Debug, Clone, Args)]
pub struct EthGetFilterChangesArgs {
    /// Filter ID.
    pub filter_id: U256,
}

#[derive(Debug, Clone, Args)]
pub struct EthGetFilterLogsArgs {
    /// Filter ID.
    pub filter_id: U256,
}

#[derive(Debug, Clone, Args)]
pub struct EthNewFilterArgs {
    #[command(flatten)]
    pub filter: LogFilterObject,
}

#[derive(Debug, Clone, Args)]
pub struct EthCallArgs {
    #[command(flatten)]
    pub tx: TxCallObject,
    /// Block number or tag.
    #[arg(long, default_value = "latest")]
    pub block: Option<BlockNumberOrTag>,
}

#[derive(Debug, Clone, Args)]
pub struct EthEstimateGasArgs {
    #[command(flatten)]
    pub tx: TxCallObject,
    /// Block number or tag.
    #[arg(long, default_value = "latest")]
    pub block: Option<BlockNumberOrTag>,
}

#[derive(Debug, Clone, Args)]
pub struct EthSendTransactionArgs {
    #[command(flatten)]
    pub tx: TxCallObject,
}

#[derive(Debug, Clone, Args)]
pub struct EthGetLogsArgs {
    #[command(flatten)]
    pub filter: LogFilterObject,
}

#[derive(Debug, Clone, Subcommand)]
pub enum RpcMethodSubcommand {
    // No params
    #[command(name = "eth_blockNumber")]
    EthBlockNumber,
    #[command(name = "eth_chainId")]
    EthChainId,
    #[command(name = "eth_gasPrice")]
    EthGasPrice,
#[command(name = "eth_syncing")]
    EthSyncing,
    #[command(name = "eth_accounts")]
    EthAccounts,
    #[command(name = "eth_protocolVersion")]
    EthProtocolVersion,
    #[command(name = "eth_coinbase")]
    EthCoinbase,
    #[command(name = "eth_mining")]
    EthMining,
    #[command(name = "eth_hashrate")]
    EthHashrate,
    #[command(name = "eth_newBlockFilter")]
    EthNewBlockFilter,
    #[command(name = "eth_newPendingTransactionFilter")]
    EthNewPendingTransactionFilter,
    #[command(name = "net_version")]
    NetVersion,
    #[command(name = "net_listening")]
    NetListening,
    #[command(name = "net_peerCount")]
    NetPeerCount,
    #[command(name = "web3_clientVersion")]
    Web3ClientVersion,

    // Simple positional params
    #[command(name = "web3_sha3")]
    Web3Sha3(Web3Sha3Args),
    #[command(name = "eth_getBalance")]
    EthGetBalance(EthGetBalanceArgs),
    #[command(name = "eth_getTransactionCount")]
    EthGetTransactionCount(EthGetTransactionCountArgs),
    #[command(name = "eth_getCode")]
    EthGetCode(EthGetCodeArgs),
    #[command(name = "eth_getStorageAt")]
    EthGetStorageAt(EthGetStorageAtArgs),
    #[command(name = "eth_getBlockByNumber")]
    EthGetBlockByNumber(EthGetBlockByNumberArgs),
    #[command(name = "eth_getBlockByHash")]
    EthGetBlockByHash(EthGetBlockByHashArgs),
    #[command(name = "eth_getTransactionByHash")]
    EthGetTransactionByHash(EthGetTransactionByHashArgs),
    #[command(name = "eth_getTransactionByBlockHashAndIndex")]
    EthGetTransactionByBlockHashAndIndex(EthGetTransactionByBlockHashAndIndexArgs),
    #[command(name = "eth_getTransactionByBlockNumberAndIndex")]
    EthGetTransactionByBlockNumberAndIndex(EthGetTransactionByBlockNumberAndIndexArgs),
    #[command(name = "eth_getTransactionReceipt")]
    EthGetTransactionReceipt(EthGetTransactionReceiptArgs),
    #[command(name = "eth_getBlockTransactionCountByNumber")]
    EthGetBlockTransactionCountByNumber(EthGetBlockTransactionCountByNumberArgs),
    #[command(name = "eth_getBlockTransactionCountByHash")]
    EthGetBlockTransactionCountByHash(EthGetBlockTransactionCountByHashArgs),
    #[command(name = "eth_getUncleCountByBlockHash")]
    EthGetUncleCountByBlockHash(EthGetUncleCountByBlockHashArgs),
    #[command(name = "eth_getUncleCountByBlockNumber")]
    EthGetUncleCountByBlockNumber(EthGetUncleCountByBlockNumberArgs),
    #[command(name = "eth_getUncleByBlockHashAndIndex")]
    EthGetUncleByBlockHashAndIndex(EthGetUncleByBlockHashAndIndexArgs),
    #[command(name = "eth_getUncleByBlockNumberAndIndex")]
    EthGetUncleByBlockNumberAndIndex(EthGetUncleByBlockNumberAndIndexArgs),
    #[command(name = "eth_sign")]
    EthSign(EthSignArgs),
    #[command(name = "eth_signTransaction")]
    EthSignTransaction(EthSignTransactionArgs),
    #[command(name = "eth_sendRawTransaction")]
    EthSendRawTransaction(EthSendRawTransactionArgs),
#[command(name = "eth_uninstallFilter")]
    EthUninstallFilter(EthUninstallFilterArgs),
    #[command(name = "eth_getFilterChanges")]
    EthGetFilterChanges(EthGetFilterChangesArgs),
    #[command(name = "eth_getFilterLogs")]
    EthGetFilterLogs(EthGetFilterLogsArgs),

    // Object params
    #[command(name = "eth_call")]
    EthCall(EthCallArgs),
    #[command(name = "eth_estimateGas")]
    EthEstimateGas(EthEstimateGasArgs),
#[command(name = "eth_sendTransaction")]
    EthSendTransaction(EthSendTransactionArgs),
    #[command(name = "eth_getLogs")]
    EthGetLogs(EthGetLogsArgs),
    #[command(name = "eth_newFilter")]
    EthNewFilter(EthNewFilterArgs),
}

impl RpcMethodSubcommand {
    pub fn method_name(&self) -> &'static str {
        use RpcMethodSubcommand::*;
        match self {
            EthBlockNumber => "eth_blockNumber",
            EthChainId => "eth_chainId",
            EthGasPrice => "eth_gasPrice",
EthSyncing => "eth_syncing",
            EthAccounts => "eth_accounts",
            EthProtocolVersion => "eth_protocolVersion",
            EthCoinbase => "eth_coinbase",
            EthMining => "eth_mining",
            EthHashrate => "eth_hashrate",
            EthNewBlockFilter => "eth_newBlockFilter",
            EthNewPendingTransactionFilter => "eth_newPendingTransactionFilter",
            NetVersion => "net_version",
            NetListening => "net_listening",
            NetPeerCount => "net_peerCount",
            Web3ClientVersion => "web3_clientVersion",
            Web3Sha3(_) => "web3_sha3",
            EthGetBalance(_) => "eth_getBalance",
            EthGetTransactionCount(_) => "eth_getTransactionCount",
            EthGetCode(_) => "eth_getCode",
            EthGetStorageAt(_) => "eth_getStorageAt",
            EthGetBlockByNumber(_) => "eth_getBlockByNumber",
            EthGetBlockByHash(_) => "eth_getBlockByHash",
            EthGetTransactionByHash(_) => "eth_getTransactionByHash",
            EthGetTransactionByBlockHashAndIndex(_) => "eth_getTransactionByBlockHashAndIndex",
            EthGetTransactionByBlockNumberAndIndex(_) => "eth_getTransactionByBlockNumberAndIndex",
            EthGetTransactionReceipt(_) => "eth_getTransactionReceipt",
            EthGetBlockTransactionCountByNumber(_) => "eth_getBlockTransactionCountByNumber",
            EthGetBlockTransactionCountByHash(_) => "eth_getBlockTransactionCountByHash",
            EthGetUncleCountByBlockHash(_) => "eth_getUncleCountByBlockHash",
            EthGetUncleCountByBlockNumber(_) => "eth_getUncleCountByBlockNumber",
            EthGetUncleByBlockHashAndIndex(_) => "eth_getUncleByBlockHashAndIndex",
            EthGetUncleByBlockNumberAndIndex(_) => "eth_getUncleByBlockNumberAndIndex",
            EthSign(_) => "eth_sign",
            EthSignTransaction(_) => "eth_signTransaction",
            EthSendRawTransaction(_) => "eth_sendRawTransaction",
EthUninstallFilter(_) => "eth_uninstallFilter",
            EthGetFilterChanges(_) => "eth_getFilterChanges",
            EthGetFilterLogs(_) => "eth_getFilterLogs",
            EthCall(_) => "eth_call",
            EthEstimateGas(_) => "eth_estimateGas",
EthSendTransaction(_) => "eth_sendTransaction",
            EthGetLogs(_) => "eth_getLogs",
            EthNewFilter(_) => "eth_newFilter",
        }
    }

    pub fn to_params(&self) -> serde_json::Value {
        use RpcMethodSubcommand::*;

        let params = match self {
            // No params
            EthBlockNumber
            | EthChainId
            | EthGasPrice
| EthSyncing
            | EthAccounts
            | EthProtocolVersion
            | EthCoinbase
            | EthMining
            | EthHashrate
            | EthNewBlockFilter
            | EthNewPendingTransactionFilter
            | NetVersion
            | NetListening
            | NetPeerCount
            | Web3ClientVersion => vec![],

            // Single param
            Web3Sha3(a) => single_param(&a.data),
            EthGetTransactionByHash(a) => single_param(&a.hash),
            EthGetTransactionReceipt(a) => single_param(&a.hash),
            EthGetBlockTransactionCountByNumber(a) => single_param(&a.block),
            EthGetBlockTransactionCountByHash(a) => single_param(&a.hash),
            EthGetUncleCountByBlockHash(a) => single_param(&a.hash),
            EthGetUncleCountByBlockNumber(a) => single_param(&a.block),
            EthSendRawTransaction(a) => single_param(&a.data),
            EthUninstallFilter(a) => single_param(&a.filter_id),
            EthGetFilterChanges(a) => single_param(&a.filter_id),
            EthGetFilterLogs(a) => single_param(&a.filter_id),

            // address + optional block
            EthGetBalance(a) => addr_block_params(&a.address, &a.block),
            EthGetTransactionCount(a) => addr_block_params(&a.address, &a.block),
            EthGetCode(a) => addr_block_params(&a.address, &a.block),

            // Two positional params
            EthSign(a) => {
                let mut out = Vec::with_capacity(2);
                push_param(&mut out, &a.address);
                push_param(&mut out, &a.message);
                out
            }
            EthGetTransactionByBlockHashAndIndex(a) => {
                let mut out = Vec::with_capacity(2);
                push_param(&mut out, &a.block_hash);
                push_param(&mut out, &a.index);
                out
            }
            EthGetTransactionByBlockNumberAndIndex(a) => {
                let mut out = Vec::with_capacity(2);
                push_param(&mut out, &a.block);
                push_param(&mut out, &a.index);
                out
            }
            EthGetUncleByBlockHashAndIndex(a) => {
                let mut out = Vec::with_capacity(2);
                push_param(&mut out, &a.block_hash);
                push_param(&mut out, &a.index);
                out
            }
            EthGetUncleByBlockNumberAndIndex(a) => {
                let mut out = Vec::with_capacity(2);
                push_param(&mut out, &a.block);
                push_param(&mut out, &a.index);
                out
            }

            EthGetStorageAt(a) => {
                let mut out = Vec::with_capacity(3);
                push_param(&mut out, &a.address);
                push_param(&mut out, &a.position);
                push_optional(&mut out, &a.block);
                out
            }
            EthGetBlockByNumber(a) => {
                let mut out = Vec::with_capacity(2);
                push_param(&mut out, &a.block);
                push_param(&mut out, &a.full_txs);
                out
            }
            EthGetBlockByHash(a) => {
                let mut out = Vec::with_capacity(2);
                push_param(&mut out, &a.hash);
                push_param(&mut out, &a.full_txs);
                out
            }

// tx call object + optional block
            EthCall(a) => tx_object_params(&a.tx, &a.block),
            EthEstimateGas(a) => tx_object_params(&a.tx, &a.block),
EthSignTransaction(a) => tx_object_params(&a.tx, &None),
            EthSendTransaction(a) => tx_object_params(&a.tx, &None),

            // Filter object
            EthGetLogs(a) => single_param(&a.filter),
            EthNewFilter(a) => single_param(&a.filter),
        };

        serde_json::Value::Array(params)
    }
}

fn single_param(val: &impl Serialize) -> Vec<serde_json::Value> {
    let mut out = Vec::with_capacity(1);
    push_param(&mut out, val);
    out
}

fn addr_block_params(
    address: &Address,
    block: &Option<BlockNumberOrTag>,
) -> Vec<serde_json::Value> {
    let mut out = Vec::with_capacity(2);
    push_param(&mut out, address);
    push_optional(&mut out, block);
    out
}

pub async fn run_rpc_spam(
    args: RpcCliArgs,
    db: &SqliteDb,
    data_dir: &Path,
) -> Result<(), CliError> {
    let client = ClientBuilder::default()
        .layer(LoggingLayer::new(&PROM, &LATENCY_HIST).await)
        .http(args.rpc_url.clone());
    let provider = DynProvider::new(
        ProviderBuilder::new()
            .network::<AnyNetwork>()
            .connect_client(client),
    );

    let method = args.method.method_name();
    let params = args.method.to_params();
    let rps = args.rps;
    let duration = args.duration;
    let total = rps * duration;

    println!("Spamming {method} at {rps} rps for {duration}s ({total} total requests)");

    let interval = Duration::from_secs_f64(1.0 / rps as f64);
    let mut tick = tokio::time::interval(interval);
    tick.tick().await; // consume the immediate first tick
    tick.set_missed_tick_behavior(MissedTickBehavior::Burst);

    let mut tasks = JoinSet::new();

    for _ in 0..total {
        tick.tick().await;
        let provider = provider.clone();
        let params = params.clone();
        tasks.spawn(async move {
            provider
                .raw_request::<_, serde_json::Value>(method.into(), params)
                .await
        });
    }

    let mut success: u64 = 0;
    let mut errors: u64 = 0;

    while let Some(result) = tasks.join_next().await {
        match result {
            Ok(Ok(_)) => success += 1,
            Ok(Err(e)) => {
                errors += 1;
                warn!("RPC error: {e}");
            }
            Err(e) => {
                errors += 1;
                warn!("Task join error: {e}");
            }
        }
    }

    println!("\n--- RPC Spam Summary ---");
    println!("Method:    {method}");
    println!("Total:     {total}");
    println!("Success:   {success}");
    println!("Errors:    {errors}");

    print_latency_summary(method);

    // Persist run + latency metrics to DB
    let latency_map = collect_latency_metrics();
    let run_id = db
        .insert_run(&SpamRunRequest {
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as usize,
            tx_count: 0,
            scenario_name: format!("rpc:{method}"),
            campaign_id: None,
            campaign_name: None,
            stage_name: None,
            rpc_url: args.rpc_url.to_string(),
            txs_per_duration: rps,
            duration: SpamDuration::Seconds(duration),
            pending_timeout: Duration::from_secs(0),
        })
        .map_err(CliError::Db)?;
    db.insert_latency_metrics(run_id, &latency_map)
        .map_err(CliError::Db)?;
    info!("Saved RPC spam run #{run_id}");

    if args.gen_report {
        generate_rpc_report(
            method,
            &args,
            run_id,
            total,
            success,
            errors,
            &latency_map,
            data_dir,
        )?;
    }

    Ok(())
}

fn generate_rpc_report(
    method: &str,
    args: &RpcCliArgs,
    run_id: u64,
    total: u64,
    success: u64,
    errors: u64,
    latency_map: &BTreeMap<String, Vec<Bucket>>,
    data_dir: &Path,
) -> Result<(), CliError> {
    let buckets = latency_map.get(method).cloned().unwrap_or_default();
    let latency_chart = LatencyChart::new(buckets.clone());

    let to_ms = |v: f64| (v * 1000.0).round() as u64;
    use contender_core::buckets::BucketsExt;
    let quantiles = RpcLatencyQuantiles {
        p50: to_ms(buckets.estimate_quantile(0.5)),
        p90: to_ms(buckets.estimate_quantile(0.9)),
        p99: to_ms(buckets.estimate_quantile(0.99)),
        method: method.to_owned(),
    };

    let meta = RpcReportMetadata {
        method: method.to_owned(),
        rpc_url: args.rpc_url.to_string(),
        run_id,
        rps: args.rps,
        duration_secs: args.duration,
        total_requests: total,
        success_count: success,
        error_count: errors,
        latency_quantiles: quantiles,
        latency_chart: latency_chart.echart_data(),
    };

    contender_report::command::report_rpc(meta, data_dir).map_err(CliError::Report)
}

fn collect_latency_metrics() -> BTreeMap<String, Vec<Bucket>> {
    match PROM.get() {
        Some(registry) => contender_core::buckets::collect_latency_from_registry(registry),
        None => BTreeMap::new(),
    }
}

fn print_latency_summary(method: &str) {
    let Some(hist) = LATENCY_HIST.get() else {
        return;
    };

    for mf in &Collector::collect(hist) {
        for m in mf.get_metric() {
            let is_our_method = m
                .get_label()
                .iter()
                .any(|l| l.name() == "rpc_method" && l.value() == method);
            if !is_our_method {
                continue;
            }

            let h = m.get_histogram();
            let count = h.get_sample_count();
            if count == 0 {
                return;
            }

            let avg = h.get_sample_sum() / count as f64;
            println!("Latency:");
            println!("  count:   {count}");
            println!("  avg:     {avg:.4}s");

            let buckets = h.get_bucket();
            for (label, quantile) in [("p50", 0.5), ("p90", 0.9), ("p95", 0.95), ("p99", 0.99)] {
                let target = (quantile * count as f64) as u64;
                if let Some(b) = buckets.iter().find(|b| b.cumulative_count() >= target) {
                    println!("  {label}:    {:.4}s", b.upper_bound());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::node_bindings::Anvil;
    use contender_sqlite::SqliteDb;
    use std::str::FromStr;

    fn spawn_anvil() -> alloy::node_bindings::AnvilInstance {
        Anvil::new().block_time_f64(0.25).spawn()
    }

    fn test_db() -> (SqliteDb, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db = SqliteDb::from_file(dir.path().join("test.db")).unwrap();
        db.create_tables().unwrap();
        (db, dir)
    }

    #[tokio::test]
    async fn rpc_spam_no_params() {
        let anvil = spawn_anvil();
        let (db, dir) = test_db();
        let args = RpcCliArgs {
            rpc_url: Url::from_str(&anvil.endpoint()).unwrap(),
            rps: 5,
            duration: 1,
            gen_report: false,
            method: RpcMethodSubcommand::EthBlockNumber,
        };
        run_rpc_spam(args, &db, dir.path()).await.unwrap();
    }

    #[tokio::test]
    async fn rpc_spam_with_positional_params() {
        let anvil = spawn_anvil();
        let (db, dir) = test_db();
        let args = RpcCliArgs {
            rpc_url: Url::from_str(&anvil.endpoint()).unwrap(),
            rps: 5,
            duration: 1,
            gen_report: false,
            method: RpcMethodSubcommand::EthGetBalance(EthGetBalanceArgs {
                address: Address::ZERO,
                block: Some(BlockNumberOrTag::Latest),
            }),
        };
        run_rpc_spam(args, &db, dir.path()).await.unwrap();
    }

    #[tokio::test]
    async fn rpc_spam_with_object_params() {
        let anvil = spawn_anvil();
        let (db, dir) = test_db();
        let args = RpcCliArgs {
            rpc_url: Url::from_str(&anvil.endpoint()).unwrap(),
            rps: 5,
            duration: 1,
            gen_report: false,
            method: RpcMethodSubcommand::EthCall(EthCallArgs {
                tx: TxCallObject {
                    from: Address::ZERO,
                    to: Address::ZERO,
                    input: Bytes::default(),
                    gas: None,
                    gas_price: None,
                    value: None,
                    nonce: None,
                },
                block: Some(BlockNumberOrTag::Latest),
            }),
        };
        run_rpc_spam(args, &db, dir.path()).await.unwrap();
    }

    #[tokio::test]
    async fn rpc_spam_get_block_by_number() {
        let anvil = spawn_anvil();
        let (db, dir) = test_db();
        let args = RpcCliArgs {
            rpc_url: Url::from_str(&anvil.endpoint()).unwrap(),
            rps: 5,
            duration: 1,
            gen_report: false,
            method: RpcMethodSubcommand::EthGetBlockByNumber(EthGetBlockByNumberArgs {
                block: BlockNumberOrTag::Latest,
                full_txs: false,
            }),
        };
        run_rpc_spam(args, &db, dir.path()).await.unwrap();
    }


#[tokio::test]
    async fn rpc_spam_get_logs() {
        let anvil = spawn_anvil();
        let (db, dir) = test_db();
        let args = RpcCliArgs {
            rpc_url: Url::from_str(&anvil.endpoint()).unwrap(),
            rps: 5,
            duration: 1,
            gen_report: false,
            method: RpcMethodSubcommand::EthGetLogs(EthGetLogsArgs {
                filter: LogFilterObject {
                    from_block: Some(BlockNumberOrTag::Earliest),
                    to_block: Some(BlockNumberOrTag::Latest),
                    address: None,
                    topics: None,
                    block_hash: None,
                },
            }),
        };
        run_rpc_spam(args, &db, dir.path()).await.unwrap();
    }
}
