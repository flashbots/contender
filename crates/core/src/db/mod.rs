mod mock;
use std::{collections::BTreeMap, fmt::Display, time::Duration};

pub use mock::MockDb;

use crate::{buckets::Bucket, Result};
use alloy::primitives::{Address, FixedBytes, TxHash};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Clone)]
pub struct RunTx {
    pub tx_hash: TxHash,
    #[serde(rename = "start_time")]
    pub start_timestamp_secs: u64,
    #[serde(rename = "end_time")]
    pub end_timestamp_secs: Option<u64>,
    pub block_number: Option<u64>,
    pub gas_used: Option<u64>,
    pub kind: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct NamedTx {
    pub name: String,
    pub tx_hash: TxHash,
    pub address: Option<Address>,
}

impl NamedTx {
    pub fn new(name: String, tx_hash: TxHash, address: Option<Address>) -> Self {
        Self {
            name,
            tx_hash,
            address,
        }
    }
}

pub enum SpamDuration {
    Seconds(u64),
    Blocks(u64),
}

impl SpamDuration {
    pub fn value(&self) -> u64 {
        match self {
            SpamDuration::Seconds(v) => *v,
            SpamDuration::Blocks(v) => *v,
        }
    }

    pub fn unit(&self) -> &'static str {
        match self {
            SpamDuration::Seconds(_) => "seconds",
            SpamDuration::Blocks(_) => "blocks",
        }
    }

    pub fn is_seconds(&self) -> bool {
        matches!(self, SpamDuration::Seconds(_))
    }

    pub fn is_blocks(&self) -> bool {
        matches!(self, SpamDuration::Blocks(_))
    }
}

impl Display for SpamDuration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpamDuration::Seconds(v) => write!(f, "{v} seconds"),
            SpamDuration::Blocks(v) => write!(f, "{v} blocks"),
        }
    }
}

impl From<String> for SpamDuration {
    fn from(value: String) -> Self {
        let value = value.trim();
        if let Some(stripped) = value.strip_suffix(" seconds") {
            if let Ok(seconds) = stripped.trim().parse::<u64>() {
                return SpamDuration::Seconds(seconds);
            }
        } else if let Some(stripped) = value.strip_suffix(" blocks") {
            if let Ok(blocks) = stripped.trim().parse::<u64>() {
                return SpamDuration::Blocks(blocks);
            }
        }
        panic!("Invalid format for SpamDuration: {value}");
    }
}

pub struct SpamRun {
    pub id: u64,
    pub timestamp: usize,
    pub tx_count: usize,
    pub scenario_name: String,
    pub rpc_url: String,
    pub txs_per_duration: u64,
    pub duration: SpamDuration,
    pub timeout: u64,
}

pub struct SpamRunRequest {
    pub timestamp: usize,
    pub tx_count: usize,
    pub scenario_name: String,
    pub rpc_url: String,
    pub txs_per_duration: u64,
    pub duration: SpamDuration,
    pub pending_timeout: Duration,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ReplayReportRequest {
    pub rpc_url_id: u64,
    pub gas_per_second: u64,
    pub gas_used: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ReplayReport {
    pub id: u64,
    #[serde(flatten)]
    req: ReplayReportRequest,
}

impl ReplayReport {
    pub fn new(id: u64, req: ReplayReportRequest) -> Self {
        Self { id, req }
    }

    pub fn gas_used(&self) -> u64 {
        self.req.gas_used
    }

    pub fn gas_per_second(&self) -> u64 {
        self.req.gas_per_second
    }

    pub fn rpc_url_id(&self) -> u64 {
        self.req.rpc_url_id
    }
}

pub trait DbOps {
    fn create_tables(&self) -> Result<()>;

    fn get_latency_metrics(&self, run_id: u64, method: &str) -> Result<Vec<Bucket>>;

    fn get_named_tx(
        &self,
        name: &str,
        rpc_url: &str,
        genesis_hash: FixedBytes<32>,
    ) -> Result<Option<NamedTx>>;

    fn get_named_tx_by_address(&self, address: &Address) -> Result<Option<NamedTx>>;

    fn get_run(&self, run_id: u64) -> Result<Option<SpamRun>>;

    fn get_run_txs(&self, run_id: u64) -> Result<Vec<RunTx>>;

    /// Insert a new named tx into the database. Used for named contracts.
    fn insert_named_txs(
        &self,
        named_txs: &[NamedTx],
        rpc_url: &str,
        genesis_hash: FixedBytes<32>,
    ) -> Result<()>;

    /// Insert a new run into the database. Returns run_id.
    fn insert_run(&self, run: &SpamRunRequest) -> Result<u64>;

    /// Insert txs from a spam run into the database.
    fn insert_run_txs(&self, run_id: u64, run_txs: &[RunTx]) -> Result<()>;

    /// Insert latency metrics into the database.
    ///
    /// `latency_metrics` maps upper_bound latency (in ms) to the number of txs that received a response within that duration.
    /// Meant to be used as input to a histogram.
    fn insert_latency_metrics(
        &self,
        run_id: u64,
        latency_metrics: &BTreeMap<String, Vec<Bucket>>,
    ) -> Result<()>;

    fn num_runs(&self) -> Result<u64>;

    /// Get the maximum txs_per_duration for a given scenario name
    fn get_max_txs_per_duration_for_scenario(&self, scenario_name: &str) -> Result<Option<u64>>;

    /// Get the RPC URL for a given scenario name (from the most recent run)
    fn get_rpc_url_for_scenario(&self, scenario_name: &str) -> Result<Option<String>>;

    fn version(&self) -> u64;

    /// Returns the COUNT of the replay_reports table. Used to get the `id` for inserting a new report.
    fn num_replay_reports(&self) -> Result<u64>;

    /// Insert a new replay report into the `replay_reports` table.
    fn insert_replay_report(&self, report: ReplayReportRequest) -> Result<ReplayReport>;

    /// Get a replay report by its `id`.
    fn get_replay_report(&self, id: u64) -> Result<ReplayReport>;

    /// Get id for a given RPC URL and genesis hash. Adds to DB if not present.
    fn get_rpc_url_id(&self, rpc_url: impl AsRef<str>, genesis_hash: FixedBytes<32>)
        -> Result<u64>;
}
