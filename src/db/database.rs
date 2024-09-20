use alloy::primitives::{Address, TxHash};

use crate::Result;

pub trait DbOps {
    fn create_tables(&self) -> Result<()>;

    fn insert_run(&self, timestamp: u64, tx_count: usize) -> Result<usize>;

    fn num_runs(&self) -> Result<i64>;

    fn insert_named_tx(
        &self,
        name: String,
        tx_hash: TxHash,
        contract_address: Option<Address>,
    ) -> Result<()>;

    fn get_named_tx(&self, name: &str) -> Result<(TxHash, Option<Address>)>;

    fn insert_run_tx(&self, run_id: i64, tx_hash: TxHash, timestamp: usize) -> Result<()>;
}
