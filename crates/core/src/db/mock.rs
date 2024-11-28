use alloy::primitives::{Address, TxHash};

use super::{DbOps, NamedTx, RunTx};
use crate::Result;

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

    fn get_named_tx(&self, _name: &str) -> Result<Option<NamedTx>> {
        Ok(Some(NamedTx::new(
            String::default(),
            TxHash::default(),
            None,
        )))
    }

    fn get_named_tx_by_address(&self, address: &Address) -> Result<Option<NamedTx>> {
        Ok(Some(NamedTx::new(
            String::default(),
            TxHash::default(),
            Some(*address),
        )))
    }

    fn insert_run_txs(&self, _run_id: u64, _run_txs: Vec<RunTx>) -> Result<()> {
        Ok(())
    }

    fn get_run_txs(&self, _run_id: u64) -> Result<Vec<RunTx>> {
        Ok(vec![])
    }
}