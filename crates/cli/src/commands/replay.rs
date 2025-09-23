use std::time::{Duration, Instant};

use crate::commands::common::{AuthCliArgs, EngineParams};
use contender_core::error::ContenderError;
use tracing::info;

#[derive(Clone, Debug, clap::Args)]
pub struct ReplayCliArgs {
    // authenticated engine_ API
    #[command(flatten)]
    auth_params: AuthCliArgs,

    /// The first block to start replaying.
    start_block: u64,
}

#[derive(Clone)]
pub struct ReplayArgs {
    engine_params: EngineParams,
    start_block: u64,
}

impl ReplayArgs {
    pub fn new(engine_params: EngineParams, start_block: u64) -> Self {
        Self {
            engine_params,
            start_block,
        }
    }

    pub async fn from_cli_args(args: ReplayCliArgs) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self::new(
            args.auth_params.engine_params().await?,
            args.start_block,
        ))
    }
}

pub async fn replay(args: ReplayArgs) -> Result<(), Box<dyn std::error::Error>> {
    info!("rewinding to block {}...", args.start_block);

    let engine_provider =
        args.engine_params
            .engine_provider
            .ok_or(ContenderError::InvalidRuntimeParams(
                contender_core::error::RuntimeParamErrorKind::MissingArgs(
                    "engine_provider is required for replay".to_owned(),
                ),
            ))?;

    let start_timestamp = Instant::now();
    engine_provider
        .replay_chain_segment(args.start_block)
        .await?;
    let time_elapsed = Instant::now().duration_since(start_timestamp);

    let time_elapsed_str = if time_elapsed > Duration::from_secs(1) {
        format!("{} seconds", time_elapsed.as_secs_f32())
    } else {
        format!("{} milliseconds", time_elapsed.as_millis())
    };
    info!("finished in {time_elapsed_str}");

    Ok(())
}
