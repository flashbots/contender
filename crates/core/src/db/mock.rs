use std::collections::BTreeMap;

use alloy::primitives::{Address, TxHash};

use super::{DbOps, NamedTx, RunTx};
use crate::{buckets::Bucket, Result};

pub struct MockDb;

impl DbOps for MockDb {
    fn version(&self) -> u64 {
        u64::MAX
    }

    fn create_tables(&self) -> Result<()> {
        Ok(())
    }

    fn insert_run(&self, _timestamp: u64, _tx_count: u64, _scenario_name: &str) -> Result<u64> {
        Ok(0)
    }

    fn get_run(&self, _run_id: u64) -> Result<Option<super::SpamRun>> {
        Ok(None)
    }

    fn num_runs(&self) -> Result<u64> {
        Ok(0)
    }

    fn insert_named_txs(&self, _named_txs: &[NamedTx], _rpc_url: &str) -> Result<()> {
        Ok(())
    }

    fn get_named_tx(&self, _name: &str, _rpc_url: &str) -> Result<Option<NamedTx>> {
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

    fn get_latency_metrics(&self, _run_id: u64, _method: &str) -> Result<Vec<Bucket>> {
        Ok(vec![(0.0, 1).into()])
    }

    fn insert_run_txs(&self, _run_id: u64, _run_txs: &[RunTx]) -> Result<()> {
        Ok(())
    }

    fn get_run_txs(&self, _run_id: u64) -> Result<Vec<RunTx>> {
        Ok(vec![])
    }

    fn insert_latency_metrics(
        &self,
        _run_id: u64,
        _latency_metrics: &BTreeMap<String, Vec<Bucket>>,
    ) -> Result<()> {
        Ok(())
    }
}
