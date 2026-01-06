use super::{DbOps, NamedTx, RunTx, SpamRunRequest};
use crate::{buckets::Bucket, db::DbError};
use alloy::primitives::{Address, FixedBytes, TxHash};
use std::collections::BTreeMap;

pub struct MockDb;

#[derive(Debug)]
pub enum MockError {}

impl std::fmt::Display for MockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "mock error")
    }
}

impl std::error::Error for MockError {}

impl From<MockError> for DbError {
    fn from(value: MockError) -> Self {
        DbError::Internal(value.to_string())
    }
}

impl From<MockError> for crate::Error {
    fn from(value: MockError) -> Self {
        Self::Db(value.into())
    }
}

impl DbOps for MockDb {
    type Error = MockError;

    fn get_rpc_url_id(
        &self,
        _rpc_url: impl AsRef<str>,
        _genesis_hash: FixedBytes<32>,
    ) -> Result<u64, Self::Error> {
        Ok(0)
    }

    fn get_replay_report(&self, id: u64) -> Result<super::ReplayReport, Self::Error> {
        Ok(super::ReplayReport::new(
            id,
            super::ReplayReportRequest::new(),
        ))
    }

    fn insert_replay_report(
        &self,
        req: super::ReplayReportRequest,
    ) -> Result<super::ReplayReport, Self::Error> {
        Ok(super::ReplayReport::new(0, req))
    }

    fn num_replay_reports(&self) -> Result<u64, Self::Error> {
        Ok(0)
    }

    fn get_rpc_url_for_scenario(
        &self,
        _scenario_name: &str,
    ) -> Result<Option<String>, Self::Error> {
        Ok(Some("http://localhost:8545".to_string()))
    }

    fn version(&self) -> u64 {
        u64::MAX
    }

    fn create_tables(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn insert_run(&self, _run: &SpamRunRequest) -> Result<u64, Self::Error> {
        Ok(0)
    }

    fn get_run(&self, _run_id: u64) -> Result<Option<super::SpamRun>, Self::Error> {
        Ok(None)
    }

    fn latest_campaign_id(&self) -> Result<Option<String>, Self::Error> {
        Ok(None)
    }

    fn get_runs_by_campaign(&self, _campaign_id: &str) -> Result<Vec<super::SpamRun>, Self::Error> {
        Ok(vec![])
    }

    fn num_runs(&self) -> Result<u64, Self::Error> {
        Ok(0)
    }

    fn insert_named_txs(
        &self,
        _named_txs: &[NamedTx],
        _rpc_url: &str,
        _genesis_hash: FixedBytes<32>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn get_named_tx(
        &self,
        _name: &str,
        _rpc_url: &str,
        _genesis_hash: FixedBytes<32>,
    ) -> Result<Option<NamedTx>, Self::Error> {
        Ok(Some(NamedTx::new(
            String::default(),
            TxHash::default(),
            None,
        )))
    }

    fn get_named_txs(
        &self,
        _name: &str,
        _rpc_url: &str,
        _genesis_hash: FixedBytes<32>,
    ) -> Result<Vec<NamedTx>, Self::Error> {
        Ok(vec![])
    }

    fn get_setup_progress(
        &self,
        _scenario_hash: FixedBytes<32>,
        _genesis_hash: FixedBytes<32>,
    ) -> Result<Option<u64>, Self::Error> {
        Ok(None)
    }

    fn update_setup_progress(
        &self,
        _scenario_hash: FixedBytes<32>,
        _genesis_hash: FixedBytes<32>,
        _step_index: u64,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn get_named_tx_by_address(&self, address: &Address) -> Result<Option<NamedTx>, Self::Error> {
        Ok(Some(NamedTx::new(
            String::default(),
            TxHash::default(),
            Some(*address),
        )))
    }

    fn get_latency_metrics(&self, _run_id: u64, _method: &str) -> Result<Vec<Bucket>, Self::Error> {
        Ok(vec![(0.0, 1).into()])
    }

    fn insert_run_txs(&self, _run_id: u64, _run_txs: &[RunTx]) -> Result<(), Self::Error> {
        Ok(())
    }

    fn get_run_txs(&self, _run_id: u64) -> Result<Vec<RunTx>, Self::Error> {
        Ok(vec![])
    }

    fn insert_latency_metrics(
        &self,
        _run_id: u64,
        _latency_metrics: &BTreeMap<String, Vec<Bucket>>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
}
