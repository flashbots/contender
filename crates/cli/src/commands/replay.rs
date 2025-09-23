use std::time::Duration;

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
            args.auth_params.engine_params(true).await?,
            args.start_block,
        ))
    }
}

pub async fn replay(args: ReplayArgs) -> Result<(), Box<dyn std::error::Error>> {
    let engine_provider =
        args.engine_params
            .engine_provider
            .ok_or(ContenderError::InvalidRuntimeParams(
                contender_core::error::RuntimeParamErrorKind::MissingArgs(
                    "engine_provider is required for replay".to_owned(),
                ),
            ))?;

    let res = engine_provider
        .replay_chain_segment(args.start_block)
        .await?;

    let time_elapsed_str = if res.time_elapsed > Duration::from_secs(1) {
        format!("{} seconds", res.time_elapsed.as_secs_f32())
    } else {
        format!("{} milliseconds", res.time_elapsed.as_millis())
    };
    info!("finished in {time_elapsed_str}.");
    let gas_per_sec = res.gas_per_second();
    let (gas_unit, divisor) = if gas_per_sec >= 1_000_000_000 {
        ("Ggas", 1_000_000_000.0)
    } else if gas_per_sec >= 1_000_000 {
        ("Mgas", 1_000_000.0)
    } else if gas_per_sec >= 1_000 {
        ("Kgas", 1_000.0)
    } else {
        ("gas", 1.0)
    };
    info!(
        "average engine speed: {} {gas_unit}/second",
        gas_per_sec as f64 / divisor
    );

    Ok(())
}
