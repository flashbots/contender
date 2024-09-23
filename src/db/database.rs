use alloy::primitives::{Address, TxHash};

use crate::Result;

pub struct RunTx {
    pub tx_hash: TxHash,
    pub timestamp: usize,
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

    fn insert_run_tx(&self, run_id: u64, tx_hash: TxHash, timestamp: usize) -> Result<()>;

    fn get_run_txs(&self, run_id: u64) -> Result<Vec<RunTx>>;
}
