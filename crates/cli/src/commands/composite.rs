use std::{fmt::Debug, sync::Arc};

use contender_composefile::composefile::{ComposeFile, CompositeSpamConfiguration};
use tokio::{sync::Mutex, task};
use tracing::{error, info};

use crate::commands::{setup, spam, SetupCommandArgs, SpamCommandArgs};

#[derive(Debug, clap::Args)]
pub struct CompositeScenarioArgs {
    pub filename: Option<String>,
    pub private_keys: Option<Vec<String>>,
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
    let sharable_private_keys = Arc::new(Mutex::new(args.private_keys));

    let setup_scenarios = compose_file.get_setup_config()?;
    let setup_tasks: Vec<_> = setup_scenarios
        .into_iter()
        .enumerate()
        .map(|(index, scenario)| {
            let db_clone = sharable_db.clone();
            let scenario_config = scenario.clone();
            let private_keys = sharable_private_keys.clone();
            let setup_command_args = SetupCommandArgs::from_json(scenario_config.config);

            task::spawn(async move {
                let setup_command = setup_command_args
                    .await
                    .expect("msg")
                    .with_private_keys(private_keys.lock().await.clone());
                match setup(&*db_clone.lock().await, setup_command).await {
                    Ok(_) => info!(
                        "Setup [{index}] - {}: completed successfully",
                        &scenario_config.name
                    ),
                    Err(err) => error!(
                        "Setup [{index}] - {} failed: {err:?}",
                        &scenario_config.name
                    ),
                };
                //setup(&*db_clone.lock().await, scenario_config.config).await.map_err(|e| Err("s".into()))
            })
        })
        .collect();

    futures::future::join_all(setup_tasks).await;

    info!("================================================================================================= Done Composite run for setup =================================================================================================");

    let spam_scenarios = compose_file.get_spam_config()?;
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
            let private_keys = sharable_private_keys.clone();
            let private_keys_clone = private_keys.clone().lock().await.clone();
            let task = task::spawn(async move {
                let spam_call = async || -> Result<(), Box<dyn std::error::Error>> {
                    let spam_command_args = SpamCommandArgs::from_json(spam_command)
                        .await?
                        .with_private_keys(private_keys_clone);

                    let mut test_scenario = spam_command_args
                        .init_scenario(&*db_clone.lock().await)
                        .await?;
                    spam(
                        &*db_clone.lock().await,
                        &spam_command_args,
                        &mut test_scenario,
                    )
                    .await?;
                    Ok(())
                };

                match spam_call().await {
                    Ok(()) => {
                        info!("Successful: Scenario [{spam_scenario_index:?}]");
                    }
                    Err(err) => {
                        error!("Error occured while Scenario [{spam_scenario_index:?}]: {err:?}")
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
    Ok(())
}
