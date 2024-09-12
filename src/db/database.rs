use alloy::primitives::{Address, TxHash};

use crate::Result;

pub trait DbOps {
    fn create_tables(&self) -> Result<()>;

    fn insert_run(&self, timestamp: &str, tx_count: i64, duration: i64) -> Result<()>;

    fn num_runs(&self) -> Result<i64>;

    fn insert_named_tx(
        &self,
        name: String,
        tx_hash: TxHash,
        contract_address: Option<Address>,
    ) -> Result<()>;
}
