use contender_core::db::DbOps;
use csv::WriterBuilder;

use crate::util::write_run_txs;

pub enum ReportOutput {
    // Stdout,
    File(String),
}

pub fn report(
    db: &(impl DbOps + Clone + Send + Sync + 'static),
    id: Option<u64>,
    data_output: ReportOutput,
) -> Result<(), Box<dyn std::error::Error>> {
    let num_runs = db.num_runs()?;
    if num_runs == 0 {
        println!("No runs found in the database. Exiting.");
        return Ok(());
    }

    // if id is provided, check if it's valid
    let id = if let Some(id) = id {
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

    let txs = db.get_run_txs(id)?;

    match data_output {
        // ReportOutput::Stdout => {
        //     let mut writer = WriterBuilder::new()
        //         .has_headers(true)
        //         .from_writer(std::io::stdout());
        //     write_run_txs(&mut writer, &txs)?;
        // }
        ReportOutput::File(out_file) => {
            println!("Exporting report for run #{:?} to file {:?}", id, out_file);
            let mut writer = WriterBuilder::new().has_headers(true).from_path(out_file)?;
            write_run_txs(&mut writer, &txs)?;
        }
    }

    Ok(())
}
