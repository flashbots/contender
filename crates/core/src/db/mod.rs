mod mock;
pub use mock::MockDb;

use crate::Result;
use alloy::primitives::{Address, TxHash};
use serde::Serialize;

#[derive(Debug, Serialize, Clone)]
pub struct RunTx {
    pub tx_hash: TxHash,
    #[serde(rename = "start_time")]
    pub start_timestamp: u64,
    #[serde(rename = "end_time")]
    pub end_timestamp: Option<u64>,
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

pub struct SpamRun {
    pub id: u64,
    pub timestamp: usize,
    pub tx_count: usize,
    pub scenario_name: String,
}

pub trait DbOps {
    fn create_tables(&self) -> Result<()>;

    fn get_sendtx_latency(&self, run_id: u64) -> Result<Vec<(f64, u64)>>;

    fn get_named_tx(&self, name: &str, rpc_url: &str) -> Result<Option<NamedTx>>;

    fn get_named_tx_by_address(&self, address: &Address) -> Result<Option<NamedTx>>;

    fn get_run(&self, run_id: u64) -> Result<Option<SpamRun>>;

    fn get_run_txs(&self, run_id: u64) -> Result<Vec<RunTx>>;

    /// Insert a new named tx into the database. Used for named contracts.
    fn insert_named_txs(&self, named_txs: &[NamedTx], rpc_url: &str) -> Result<()>;

    /// Insert a new run into the database. Returns run_id.
    fn insert_run(&self, timestamp: u64, tx_count: u64, scenario_name: &str) -> Result<u64>;

    /// Insert txs from a spam run into the database.
    fn insert_run_txs(&self, run_id: u64, run_txs: &[RunTx]) -> Result<()>;

    /// Insert latency metrics into the database.
    ///
    /// `latency_metrics` maps upper_bound latency (in ms) to the number of txs that received a response within that duration.
    /// Meant to be used as input to a histogram.
    fn insert_latency_metrics(&self, run_id: u64, latency_metrics: &[(f64, u64)]) -> Result<()>;

    fn num_runs(&self) -> Result<u64>;

    fn version(&self) -> u64;
}
