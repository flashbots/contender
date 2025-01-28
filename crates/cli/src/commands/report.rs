use contender_core::db::{DbOps, RunTx};
use csv::WriterBuilder;

use crate::util::write_run_txs;

pub fn report(
    db: &(impl DbOps + Clone + Send + Sync + 'static),
    last_run_id: Option<u64>,
    preceding_runs: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let num_runs = db.num_runs()?;
    if num_runs == 0 {
        println!("No runs found in the database. Exiting.");
        return Ok(());
    }

    // if id is provided, check if it's valid
    let end_run_id = if let Some(id) = last_run_id {
        if id == 0 || id > num_runs {
            // panic!("Invalid run ID: {}", id);
            return Err(format!("Invalid run ID: {}", id).into());
        }
        id
    } else {
        // get latest run
        println!("No run ID provided. Using latest run ID: {}", num_runs);
        num_runs
    };

    // collect CSV report for each run_id
    let start_run_id = end_run_id - preceding_runs;
    let mut all_txs = vec![];
    for id in start_run_id..=end_run_id {
        let txs = db.get_run_txs(id)?;
        all_txs.extend_from_slice(&txs);
        save_report(id, &txs)?;
    }

    // make the high-level report
    let total_txs = all_txs.len();
    println!("total_txs: {}", total_txs);

    Ok(())
}

/// Saves txs to `~/.contender/report_{id}.csv`.
pub fn save_report(id: u64, txs: &[RunTx]) -> Result<(), Box<dyn std::error::Error>> {
    // make path to ~/.contender/report_<id>.csv
    let home_dir = std::env::var("HOME").expect("Could not get home directory");
    let contender_dir = format!("{}/.contender", home_dir);
    std::fs::create_dir_all(&contender_dir)?;
    let out_path = format!("{}/report_{}.csv", contender_dir, id);

    println!("Exporting report for run #{:?} to {:?}", id, out_path);
    let mut writer = WriterBuilder::new().has_headers(true).from_path(out_path)?;
    write_run_txs(&mut writer, txs)?;

    Ok(())
}
