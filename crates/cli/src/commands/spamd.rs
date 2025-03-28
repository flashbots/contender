use super::SpamCommandArgs;
use crate::commands::{self};
use contender_core::{db::DbOps, error::ContenderError};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

pub async fn spamd(
    db: &(impl DbOps + Clone + Send + Sync + 'static),
    args: SpamCommandArgs,
    gen_report: bool,
    time_limit: Option<u64>,
) -> Result<(), Box<dyn std::error::Error>> {
    // collects all run IDs for reporting
    let (run_id_sender, mut run_id_receiver) = tokio::sync::mpsc::channel::<u64>(1000);

    let SpamCommandArgs {
        testfile,
        rpc_url,
        builder_url,
        txs_per_block,
        txs_per_second,
        duration,
        seed,
        private_keys,
        disable_reporting,
        min_balance,
        tx_type,
        gas_price_percent_add,
    } = args;

    let finished = Arc::new(AtomicBool::new(false));
    let start_time = std::time::Instant::now();
    let rpc = rpc_url.clone();

    // spawn a task to check the time limit and set finished to true if it is reached
    let is_finished = finished.clone();
    tokio::task::spawn(async move {
        // check time limit
        if let Some(limit) = time_limit {
            tokio::time::sleep(Duration::from_secs(limit)).await;
            if start_time.elapsed().as_secs() >= limit {
                println!("Time limit reached.Spam daemon will shut down as soon as current batch finishes...");
                is_finished.store(true, Ordering::SeqCst);
            }
        }
    });

    // runs spam command in an async loop; in closure for tokio::select to handle CTRL-C
    let is_finished = finished.clone();
    let spam_loop = || async move {
        loop {
            if is_finished.load(Ordering::SeqCst) {
                println!("Spam loop finished");
                break;
            }
            let args = SpamCommandArgs {
                testfile: testfile.clone(),
                rpc_url: rpc.to_owned(),
                builder_url: builder_url.clone(),
                txs_per_block,
                txs_per_second,
                duration: duration.clone(),
                seed: seed.clone(),
                private_keys: private_keys.clone(),
                disable_reporting,
                min_balance: min_balance.clone(),
                tx_type: tx_type.into(),
                gas_price_percent_add,
            };
            let db = db.clone();
            let spam_res = commands::spam(&db, args).await;
            if let Err(e) = spam_res {
                println!("spam failed: {:?}", e);
            } else {
                println!("spam batch completed");
                let run_id = spam_res.expect("spam");
                if let Some(run_id) = run_id {
                    // run_ids.push(run_id);
                    run_id_sender
                        .send(run_id)
                        .await
                        .expect("failed to send run ID");
                }
            }
        }

        Ok::<_, ContenderError>(())
    };

    tokio::select! {
        _ = spam_loop() => {},
        _ = tokio::signal::ctrl_c() => {
            println!("CTRL-C received, stopping spam daemon...");
        }
    }

    run_id_receiver.close();

    // generate a report if requested; in closure for tokio::select to handle CTRL-C
    let run_report = || async move {
        let mut run_ids = vec![];
        while let Some(run_id) = run_id_receiver.recv().await {
            run_ids.push(run_id);
        }

        if gen_report {
            if run_ids.is_empty() {
                println!("No runs found, exiting.");
                return Ok::<_, ContenderError>(());
            }
            let first_run_id = run_ids.iter().min().expect("no run IDs found");
            let last_run_id = *run_ids.iter().max().expect("no run IDs found");
            commands::report(
                Some(last_run_id),
                last_run_id - first_run_id,
                &*db,
                &rpc_url,
            )
            .await
            .map_err(|e| {
                ContenderError::GenericError("failed to generate report", e.to_string())
            })?;
        }
        Ok(())
    };

    tokio::select! {
        _ = run_report() => {},
        _ = tokio::signal::ctrl_c() => {
            println!("CTRL-C received, shutting down...");
        }
    }

    Ok(())
}
