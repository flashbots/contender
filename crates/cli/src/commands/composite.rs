use std::{fmt::Debug, sync::Arc};

use tokio::{sync::Mutex, task};
use tracing::{error, info};

use crate::commands::{composefile::CompositeSpamConfiguration};

use super::{composefile::{ComposeFile}, setup, spam};

#[derive(Debug, clap::Args)]
pub struct CompositeScenarioArgs {
    pub filename: Option<String>,
}

pub async fn composite(
    db: &(impl contender_core::db::DbOps + Clone + Send + Sync + 'static),
    args: CompositeScenarioArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    let compose_file_name = match args.filename {
        Some(filepath) => filepath,
        None => String::from("./contender-compose.yml"),
    };

    let compose_file = ComposeFile::init_from_path(compose_file_name)?;
    let sharable_db = Arc::new(Mutex::new(db.clone()));

    if let Some(setup_scenarios) = compose_file.setup {
        let setup_tasks: Vec<_> = setup_scenarios
            .into_iter()
            .enumerate()
            .map(|(index, scenario)| {
                let db_clone = sharable_db.clone();
                let scenario_config = scenario.clone();

                task::spawn(async move {
                    let result = setup(&*db_clone.lock().await, scenario_config.config).await;
                    match &result {
                        Ok(_) => info!(
                            "Scenario [{index}] - {}: completed successfully",
                            &scenario_config.name
                        ),
                        Err(err) => error!(
                            "Scenario [{index}] - {} failed: {err:?}",
                            &scenario_config.name
                        ),
                    };
                    //setup(&*db_clone.lock().await, scenario_config.config).await.map_err(|e| Err("s".into()))
                })
            })
            .collect();

        futures::future::join_all(setup_tasks).await;

        info!("================================================================================================= Done Composite run for setup =================================================================================================");
    };

    if let Some(spam_scenarios) = compose_file.spam {
        for scenario in spam_scenarios {
            let CompositeSpamConfiguration {
                stage_name,
                spam_configs,
            } = scenario;
            info!("================================================================================================= Running stage: {stage_name:?} =================================================================================================");

            let mut spam_tasks = Vec::new();
            let sharable_stage_name_object = Arc::new(Mutex::new(stage_name.clone()));
            for (spam_scenario_index, spam_command) in spam_configs.into_iter().enumerate() {
                info!("Starting scenario [{spam_scenario_index:?}]");
                let db_clone = sharable_db.clone();
                let task = task::spawn(async move {
                    let mut test_scenario = spam_command
                        .init_scenario(&*db_clone.lock().await)
                        .await
                        .unwrap();
                    let spam_result =
                        spam(&*db_clone.lock().await, &spam_command, &mut test_scenario).await;
                    match spam_result {
                        Ok(run_id) => {
                            if let Some(run_id_value) = run_id {
                                info!("Successful: Scenario [{spam_scenario_index:?}] Run ID: [{run_id_value:?}]");
                            } else {
                                info!("Successful: Scenario [{spam_scenario_index:?}] No run ID");
                            }
                        }
                        Err(e) => {
                            error!("Error: Scenario [{spam_scenario_index:?}]: {e:?}");
                        }
                    };
                });
                spam_tasks.push(task);
            }

            for task in spam_tasks {
                task.await?;
            }
            info!("================================================================================================= Done Composite run for spam - Stage [{:?}] =================================================================================================", &*sharable_stage_name_object.clone().lock().await);
        }
    }
    Ok(())
}

