// step 1: create an API that exposes all the individual functions & metrics that are represented currently in the `report` function
// step 2: replace code in `report`` function with calls to the new API
// step 3: create new API endpoints in the RPC server that expose these metrics

use std::{
    path::{Path, PathBuf},
    str::FromStr,
};

use alloy::{
    network::{AnyNetwork, AnyRpcBlock},
    providers::{DynProvider, ProviderBuilder},
};
use contender_core::{
    db::{DbOps, RunTx, SpamRun},
    generator::types::AnyProvider,
    test_scenario::Url,
};
use csv::WriterBuilder;
use tracing::info;

use crate::{
    block_trace::{estimate_block_data, get_block_data},
    command::{ReportParams, RuntimeParams},
    util::write_run_txs,
};

pub struct ReportData<'db, D: DbOps> {
    db: &'db D,
    pub rpc_url: Url,
    /// start & end run_id
    pub run_ids: (u64, u64),
    pub rpc_client: AnyProvider,
    csv_data_dir: PathBuf,
    pub all_txs: Vec<RunTx>,
}

pub struct RunDataAndParams {
    pub run_data: Vec<SpamRun>,
    pub runtime_params: Vec<RuntimeParams>,
}

impl<'a, D: DbOps> ReportData<'a, D> {
    pub fn new(db: &'a D, params: &ReportParams, data_dir: &Path) -> crate::Result<Self> {
        let num_runs = db.num_runs().map_err(|e| e.into())?;

        if num_runs == 0 {
            info!("No runs found in the database. Exiting.");
            return Err(crate::Error::NoRunsFound);
        }

        // if id is provided, check if it's valid
        let end_run_id = if let Some(id) = params.last_run_id {
            if id == 0 || id > num_runs {
                return Err(crate::Error::InvalidRunId(id));
            }
            id
        } else {
            // get latest run
            info!("No run ID provided. Using latest run ID: {num_runs}");
            num_runs
        };
        let start_run_id = end_run_id - params.preceding_runs;

        // get rpc_url from the end run (assumes this run has the RPC URL we want to analyze)
        let rpc_url = db
            .get_run(end_run_id)
            .map_err(|e| e.into())?
            .ok_or(crate::Error::RunDoesNotExist(end_run_id))?
            .rpc_url;
        let rpc_url = Url::from_str(&rpc_url).map_err(|_| crate::Error::UrlParse(rpc_url))?;

        let rpc_client = DynProvider::new(
            ProviderBuilder::new()
                .network::<AnyNetwork>()
                .connect_http(rpc_url.clone()),
        );

        let mut all_txs = vec![];
        let reports_dir = data_dir.join("reports");
        for id in start_run_id..=end_run_id {
            let txs = db.get_run_txs(id).map_err(|e| e.into())?;
            all_txs.extend_from_slice(&txs);
            // save CSV report for each run
            save_csv_report(id, &txs, &reports_dir)?;
        }

        Ok(Self {
            db,
            rpc_url,
            rpc_client,
            run_ids: (start_run_id, end_run_id),
            csv_data_dir: data_dir.to_path_buf(),
            all_txs,
        })
    }

    pub fn run_id_range(&self) -> std::ops::Range<u64> {
        self.run_ids.0..self.run_ids.1
    }

    /// Get `SpamRun` data for a given range of run ids. `run_ids` is inclusive ([x, y])
    pub fn get_run_data(&self) -> crate::Result<RunDataAndParams> {
        let mut run_data = vec![];
        let mut runtime_params = Vec::new();
        for id in self.run_id_range() {
            let run = self.db.get_run(id).map_err(|e| e.into())?;
            if let Some(run) = run {
                if Url::from_str(&run.rpc_url)
                    .map_err(|_| crate::Error::UrlParse(run.rpc_url.clone()))?
                    != self.rpc_url
                {
                    continue;
                }
                runtime_params.push(RuntimeParams {
                    txs_per_duration: run.txs_per_duration,
                    duration_value: run.duration.value(),
                    duration_unit: run.duration.unit().to_owned(),
                    timeout: run.timeout,
                });
                run_data.push(run);
            }
        }
        Ok(RunDataAndParams {
            run_data,
            runtime_params,
        })
    }

    pub async fn estimate_block_data(&self) -> crate::Result<Vec<AnyRpcBlock>> {
        estimate_block_data(
            self.run_id_range().start,
            self.run_id_range().end,
            &self.rpc_client,
            self.db,
        )
        .await
    }

    pub async fn get_block_data(&self) -> crate::Result<Vec<AnyRpcBlock>> {
        get_block_data(&self.all_txs, &self.rpc_client).await
    }
}

/// Saves RunTxs to `{reports_dir}/{id}.csv`.
fn save_csv_report(id: u64, txs: &[RunTx], reports_dir: &Path) -> crate::Result<()> {
    let out_path = reports_dir.join(format!("{id}.csv"));

    info!("Exporting report for run #{id:?} to {out_path:?}");
    let mut writer = WriterBuilder::new().has_headers(true).from_path(out_path)?;
    write_run_txs(&mut writer, txs)?;

    Ok(())
}
