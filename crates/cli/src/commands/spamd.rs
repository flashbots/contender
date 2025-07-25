use super::SpamCommandArgs;
use crate::{
    commands::{self},
    util::data_dir,
};
use contender_core::{db::DbOps, error::ContenderError};
use std::{
    ops::Deref,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tracing::{error, info, warn};

/// Runs spam in a loop, potentially executing multiple spam runs.
///
/// If `limit_loops` is `None`, it will run indefinitely.
///
/// If `limit_loops` is `Some(n)`, it will run `n` times.
///
/// If `gen_report` is `true`, it will generate a report at the end.
pub async fn spamd(
    db: &(impl DbOps + Clone + Send + Sync + 'static),
    args: SpamCommandArgs,
    gen_report: bool,
    limit_loops: Option<u64>,
) -> Result<(), ContenderError> {
    let is_done = Arc::new(AtomicBool::new(false));
    let mut scenario = args.init_scenario(db).await?;

    // collects run IDs from the spam command
    let mut run_ids = vec![];

    // if CTRL-C signal is received, set `is_done` to true
    {
        let is_done = is_done.clone();
        tokio::task::spawn(async move {
            tokio::signal::ctrl_c()
                .await
                .expect("Failed to listen for CTRL-C");
            info!(
                "CTRL-C received. Spam daemon will shut down as soon as current batch finishes..."
            );
            is_done.store(true, Ordering::SeqCst);
        });
    }

    // runs spam command in a loop
    let mut i = 0;
    loop {
        let mut do_finish = false;
        if let Some(loops) = &limit_loops {
            if i >= *loops {
                do_finish = true;
            }
            i += 1;
        }
        if is_done.load(Ordering::SeqCst) {
            do_finish = true;
        }
        if do_finish {
            info!("Spam loop finished.");
            break;
        }

        let db = db.clone();
        let spam_res = commands::spam(&db, &args, &mut scenario).await;
        if let Err(e) = spam_res {
            error!("spam run failed: {e:?}");
            scenario.ctx.cancel_token.cancel();
            break;
        } else {
            let run_id = spam_res.expect("spam");
            if let Some(run_id) = run_id {
                run_ids.push(run_id);
            }
        }
    }

    // generate a report if requested; in closure for tokio::select to handle CTRL-C
    let run_report = || async move {
        if gen_report {
            if run_ids.is_empty() {
                warn!("No runs found, exiting.");
                return Ok::<_, ContenderError>(());
            }
            let first_run_id = run_ids.iter().min().expect("no run IDs found");
            let last_run_id = *run_ids.iter().max().expect("no run IDs found");
            contender_report::command::report(
                Some(last_run_id),
                last_run_id - first_run_id,
                db,
                &data_dir()
                    .map_err(|e| ContenderError::with_err(e.deref(), "failed to load data dir"))?,
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
            info!("CTRL-C received, cancelling report...");
        }
    }

    Ok(())
}
