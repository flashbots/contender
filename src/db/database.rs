use alloy::primitives::{Address, TxHash};
use serde::Serialize;

use crate::Result;

#[derive(Debug, Serialize)]
pub struct RunTx {
    pub tx_hash: TxHash,
    #[serde(rename = "sent_tx_hash_at")]
    pub start_timestamp: usize,
    #[serde(rename = "received_tx_hash_at")]
    pub end_timestamp: usize,
}

pub trait DbOps {
    fn create_tables(&self) -> Result<()>;

    fn insert_run(&self, timestamp: u64, tx_count: usize) -> Result<usize>;

    fn num_runs(&self) -> Result<u64>;

    fn insert_named_tx(
        &self,
        name: String,
        tx_hash: TxHash,
        contract_address: Option<Address>,
    ) -> Result<()>;

    fn get_named_tx(&self, name: &str) -> Result<(TxHash, Option<Address>)>;

    fn insert_run_tx(&self, run_id: u64, tx_hash: TxHash, start_timestamp: usize, end_timestamp: usize) -> Result<()>;

    fn get_run_txs(&self, run_id: u64) -> Result<Vec<RunTx>>;
}
