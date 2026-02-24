use crate::db::SpamDuration;
use alloy::primitives::TxHash;
use serde::Serialize;
use std::time::Duration;

#[derive(Debug, Serialize, Clone)]
pub struct RunTx {
    pub tx_hash: TxHash,
    #[serde(rename = "start_time_ms")]
    pub start_timestamp_ms: u64,
    #[serde(rename = "end_time_ms")]
    pub end_timestamp_ms: Option<u64>,
    pub block_number: Option<u64>,
    pub gas_used: Option<u64>,
    pub kind: Option<String>,
    pub error: Option<String>,
    pub flashblock_latency_ms: Option<u64>,
}

pub struct SpamRun {
    pub id: u64,
    pub timestamp: usize,
    pub tx_count: usize,
    pub scenario_name: String,
    pub campaign_id: Option<String>,
    pub campaign_name: Option<String>,
    pub stage_name: Option<String>,
    pub rpc_url: String,
    pub txs_per_duration: u64,
    pub duration: SpamDuration,
    pub timeout: u64,
}

pub struct SpamRunRequest {
    pub timestamp: usize,
    pub tx_count: usize,
    pub scenario_name: String,
    pub campaign_id: Option<String>,
    pub campaign_name: Option<String>,
    pub stage_name: Option<String>,
    pub rpc_url: String,
    pub txs_per_duration: u64,
    pub duration: SpamDuration,
    pub pending_timeout: Duration,
}
