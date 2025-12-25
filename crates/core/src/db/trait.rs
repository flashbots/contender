use std::collections::BTreeMap;

use alloy::primitives::{Address, FixedBytes};

use crate::{
    buckets::Bucket,
    db::{DbError, NamedTx, ReplayReport, ReplayReportRequest, RunTx, SpamRun, SpamRunRequest},
};

pub trait DbOps {
    type Error: Into<DbError>;

    fn create_tables(&self) -> Result<(), Self::Error>;

    fn get_latency_metrics(&self, run_id: u64, method: &str) -> Result<Vec<Bucket>, Self::Error>;

    fn get_named_tx(
        &self,
        name: &str,
        rpc_url: &str,
        genesis_hash: FixedBytes<32>,
    ) -> Result<Option<NamedTx>, Self::Error>;

    fn get_named_txs(
        &self,
        name: &str,
        rpc_url: &str,
        genesis_hash: FixedBytes<32>,
    ) -> Result<Vec<NamedTx>, Self::Error>;

    fn get_setup_progress(&self, scenario_hash: &str) -> Result<Option<u64>, Self::Error>;

    fn update_setup_progress(
        &self,
        scenario_hash: &str,
        step_index: u64,
    ) -> Result<(), Self::Error>;

    fn get_named_tx_by_address(&self, address: &Address) -> Result<Option<NamedTx>, Self::Error>;

    fn get_run(&self, run_id: u64) -> Result<Option<SpamRun>, Self::Error>;

    fn get_runs_by_campaign(&self, campaign_id: &str) -> Result<Vec<SpamRun>, Self::Error>;

    fn get_run_txs(&self, run_id: u64) -> Result<Vec<RunTx>, Self::Error>;

    /// Get latest non-null campaign_id (by run id desc).
    fn latest_campaign_id(&self) -> Result<Option<String>, Self::Error>;

    /// Insert a new named tx into the database. Used for named contracts.
    fn insert_named_txs(
        &self,
        named_txs: &[NamedTx],
        rpc_url: &str,
        genesis_hash: FixedBytes<32>,
    ) -> Result<(), Self::Error>;

    /// Insert a new run into the database. Returns run_id.
    fn insert_run(&self, run: &SpamRunRequest) -> Result<u64, Self::Error>;

    /// Insert txs from a spam run into the database.
    fn insert_run_txs(&self, run_id: u64, run_txs: &[RunTx]) -> Result<(), Self::Error>;

    /// Insert latency metrics into the database.
    ///
    /// `latency_metrics` maps upper_bound latency (in ms) to the number of txs that received a response within that duration.
    /// Meant to be used as input to a histogram.
    fn insert_latency_metrics(
        &self,
        run_id: u64,
        latency_metrics: &BTreeMap<String, Vec<Bucket>>,
    ) -> Result<(), Self::Error>;

    fn num_runs(&self) -> Result<u64, Self::Error>;

    /// Get the RPC URL for a given scenario name (from the most recent run)
    fn get_rpc_url_for_scenario(&self, scenario_name: &str) -> Result<Option<String>, Self::Error>;

    fn version(&self) -> u64;

    /// Returns the COUNT of the replay_reports table. Used to get the `id` for inserting a new report.
    fn num_replay_reports(&self) -> Result<u64, Self::Error>;

    /// Insert a new replay report into the `replay_reports` table.
    fn insert_replay_report(
        &self,
        report: ReplayReportRequest,
    ) -> Result<ReplayReport, Self::Error>;

    /// Get a replay report by its `id`.
    fn get_replay_report(&self, id: u64) -> Result<ReplayReport, Self::Error>;

    /// Get id for a given RPC URL and genesis hash. Adds to DB if not present.
    fn get_rpc_url_id(
        &self,
        rpc_url: impl AsRef<str>,
        genesis_hash: FixedBytes<32>,
    ) -> Result<u64, Self::Error>;
}
