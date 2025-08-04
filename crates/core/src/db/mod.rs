mod mock;
use std::{collections::BTreeMap, fmt::Display, time::Duration};

pub use mock::MockDb;

use crate::{buckets::Bucket, Result};
use alloy::primitives::{Address, TxHash};
use serde::Serialize;

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

pub trait DbOps {
    fn create_tables(&self) -> Result<()>;

    fn get_latency_metrics(&self, run_id: u64, method: &str) -> Result<Vec<Bucket>>;

    fn get_named_tx(&self, name: &str, rpc_url: &str) -> Result<Option<NamedTx>>;

    fn get_named_tx_by_address(&self, address: &Address) -> Result<Option<NamedTx>>;

    fn get_run(&self, run_id: u64) -> Result<Option<SpamRun>>;

    fn get_run_txs(&self, run_id: u64) -> Result<Vec<RunTx>>;

    /// Insert a new named tx into the database. Used for named contracts.
    fn insert_named_txs(&self, named_txs: &[NamedTx], rpc_url: &str) -> Result<()>;

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

    fn version(&self) -> u64;
}
