use crate::commands::common::{AuthCliArgs, EngineParams};
use crate::commands::error::ArgsError;
use crate::error::ContenderError;
use crate::util::{human_readable_duration, human_readable_gas};
use contender_core::db::{DbOps, ReplayReportRequest};
use contender_sqlite::SqliteDb;
use tracing::info;

#[derive(Clone, Debug, clap::Args)]
pub struct ReplayCliArgs {
    // authenticated engine_ API
    #[command(flatten)]
    auth_params: AuthCliArgs,

    /// The first block to start replaying.
    #[arg(
        long = "from-block",
        default_value_t = 1,
        visible_aliases = ["from"],
    )]
    start_block: u64,

    /// The last block to replay.
    #[arg(
        long = "to-block",
        visible_aliases = ["to"],
    )]
    end_block: Option<u64>,
}

#[derive(Clone)]
pub struct ReplayArgs {
    engine_params: EngineParams,
    start_block: u64,
    end_block: Option<u64>,
}

impl ReplayArgs {
    pub fn new(engine_params: EngineParams, start_block: u64, end_block: Option<u64>) -> Self {
        Self {
            engine_params,
            start_block,
            end_block,
        }
    }

    pub async fn from_cli_args(args: ReplayCliArgs) -> Result<Self, ContenderError> {
        Ok(Self::new(
            args.auth_params.engine_params(true).await?,
            args.start_block,
            args.end_block,
        ))
    }
}

pub async fn replay(args: ReplayArgs, db: &SqliteDb) -> Result<(), ContenderError> {
    let engine_provider =
        args.engine_params
            .engine_provider
            .ok_or(ArgsError::EngineProviderUninitialized(format!(
                "required for replay"
            )))?;

    let res = engine_provider
        .replay_chain_segment(args.start_block, args.end_block)
        .await?;

    let rpc_url_id =
        db.get_rpc_url_id(engine_provider.rpc_url(), engine_provider.genesis_hash())?;

    info!("finished in {}.", human_readable_duration(res.time_elapsed));
    info!(
        "average engine speed: {}/second",
        human_readable_gas(res.gas_per_second())
    );

    let report = ReplayReportRequest {
        rpc_url_id,
        gas_per_second: res.gas_per_second() as u64,
        gas_used: res.gas_used as u64,
    };
    db.insert_replay_report(report)?;

    Ok(())
}
