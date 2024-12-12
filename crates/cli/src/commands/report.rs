use contender_core::db::DbOps;
use csv::WriterBuilder;

use crate::util::write_run_txs;

pub fn report(
    db: &(impl DbOps + Clone + Send + Sync + 'static),
    id: Option<u64>,
    out_file: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let num_runs = db.num_runs()?;
    let id = if let Some(id) = id {
        if id == 0 || id > num_runs {
            panic!("Invalid run ID: {}", id);
        }
        id
    } else {
        if num_runs == 0 {
            panic!("No runs to report");
        }
        // get latest run
        println!("No run ID provided. Using latest run ID: {}", num_runs);
        num_runs
    };
    let txs = db.get_run_txs(id)?;
    println!("found {} txs", txs.len());
    println!(
        "Exporting report for run ID {:?} to out_file {:?}",
        id, out_file
    );

    if let Some(out_file) = out_file {
        let mut writer = WriterBuilder::new().has_headers(true).from_path(out_file)?;
        write_run_txs(&mut writer, &txs)?;
    } else {
        let mut writer = WriterBuilder::new()
            .has_headers(true)
            .from_writer(std::io::stdout());
        write_run_txs(&mut writer, &txs)?; // TODO: write a macro that lets us generalize the writer param to write_run_txs, then refactor this duplication
    };

    Ok(())
}
