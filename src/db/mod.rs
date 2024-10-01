use alloy::primitives::{Address, TxHash};
use serde::Serialize;

use crate::Result;

#[derive(Debug, Serialize, Clone)]
pub struct RunTx {
    pub tx_hash: TxHash,
    #[serde(rename = "start_time")]
    pub start_timestamp: usize,
    #[serde(rename = "end_time")]
    pub end_timestamp: usize,
    pub block_number: u64,
    pub gas_used: u128,
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

impl From<NamedTx> for Vec<NamedTx> {
    fn from(named_tx: NamedTx) -> Self {
        vec![named_tx]
    }
}

pub trait DbOps {
    fn create_tables(&self) -> Result<()>;

    /// Insert a new run into the database. Returns run_id.
    fn insert_run(&self, timestamp: u64, tx_count: usize) -> Result<u64>;

    fn num_runs(&self) -> Result<u64>;

    fn insert_named_txs(&self, named_txs: Vec<NamedTx>) -> Result<()>;

    fn get_named_tx(&self, name: &str) -> Result<(TxHash, Option<Address>)>;

    fn insert_run_txs(&self, run_id: u64, run_txs: Vec<RunTx>) -> Result<()>;

    fn get_run_txs(&self, run_id: u64) -> Result<Vec<RunTx>>;
}

pub struct MockDb;

impl DbOps for MockDb {
    fn create_tables(&self) -> Result<()> {
        Ok(())
    }

    fn insert_run(&self, _timestamp: u64, _tx_count: usize) -> Result<u64> {
        Ok(0)
    }

    fn num_runs(&self) -> Result<u64> {
        Ok(0)
    }

    fn insert_named_txs(&self, _named_txs: Vec<NamedTx>) -> Result<()> {
        Ok(())
    }

    fn get_named_tx(&self, _name: &str) -> Result<(TxHash, Option<Address>)> {
        Ok((TxHash::default(), None))
    }

    fn insert_run_txs(&self, _run_id: u64, _run_txs: Vec<RunTx>) -> Result<()> {
        Ok(())
    }

    fn get_run_txs(&self, _run_id: u64) -> Result<Vec<RunTx>> {
        Ok(vec![])
    }
}
