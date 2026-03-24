use crate::error::CliError;
use alloy::{
    primitives::FixedBytes,
    providers::{DynProvider, Provider, ProviderBuilder},
    rpc::client::ClientBuilder,
};
use contender_core::db::{DbOps, NamedTx};
use op_alloy_network::AnyNetwork;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::info;

use super::common::SendTxsCliArgsInner;

/// A snapshot of the scenario state after setup: contract addresses and named txs.
#[derive(Debug, Serialize, Deserialize)]
pub struct ScenarioSnapshot {
    /// The scenario file used to generate this snapshot.
    pub scenario_file: Option<String>,
    /// The RPC URL the contracts were deployed against.
    pub rpc_url: String,
    /// The genesis block hash of the chain.
    pub genesis_hash: FixedBytes<32>,
    /// All named transactions (deployed contracts, setup txs) from the DB.
    pub named_txs: Vec<NamedTx>,
}

/// CLI args for the snapshot subcommand.
#[derive(Debug, clap::Args)]
pub struct SnapshotCliArgs {
    /// The scenario file that was set up (for metadata only).
    pub testfile: Option<String>,

    #[command(flatten)]
    pub rpc_args: SendTxsCliArgsInner,

    /// Output file path for the snapshot JSON.
    #[arg(
        short,
        long,
        default_value = "snapshot.json",
        help = "Output file path for the snapshot JSON"
    )]
    pub output: PathBuf,
}

/// Export all named_txs from the DB into a snapshot JSON file.
pub async fn snapshot<D: DbOps>(
    db: &D,
    args: SnapshotCliArgs,
) -> Result<(), CliError> {
    let rpc_url = args.rpc_args.rpc_url.to_string();

    // Connect to the RPC to get the genesis hash
    let client = ClientBuilder::default().http(args.rpc_args.rpc_url.clone());
    let provider = DynProvider::new(
        ProviderBuilder::new()
            .network::<AnyNetwork>()
            .connect_client(client),
    );

    let genesis_block = provider
        .get_block_by_number(0.into())
        .await?
        .expect("genesis block not found");
    let genesis_hash = genesis_block.header.hash;

    let named_txs = db
        .get_all_named_txs(&rpc_url, genesis_hash)
        .map_err(|e| contender_core::Error::Db(e.into()))?;

    if named_txs.is_empty() {
        tracing::warn!(
            "No named transactions found in DB for RPC URL {} — snapshot will be empty.",
            rpc_url
        );
    }

    let snapshot = ScenarioSnapshot {
        scenario_file: args.testfile,
        rpc_url,
        genesis_hash,
        named_txs,
    };

    let json = serde_json::to_string_pretty(&snapshot)
        .map_err(|e| CliError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
    std::fs::write(&args.output, json)?;

    info!(
        "Snapshot saved to {} ({} named txs)",
        args.output.display(),
        snapshot.named_txs.len()
    );

    Ok(())
}

/// Load a snapshot from a JSON file and insert named_txs into the DB.
pub fn load_snapshot<D: DbOps>(
    db: &D,
    snapshot_path: &Path,
) -> Result<ScenarioSnapshot, CliError> {
    let json = std::fs::read_to_string(snapshot_path)?;
    let snapshot: ScenarioSnapshot = serde_json::from_str(&json)
        .map_err(|e| CliError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))?;

    if snapshot.named_txs.is_empty() {
        tracing::warn!("Snapshot contains no named transactions.");
        return Ok(snapshot);
    }

    db.insert_named_txs(
        &snapshot.named_txs,
        &snapshot.rpc_url,
        snapshot.genesis_hash,
    )
    .map_err(|e| contender_core::Error::Db(e.into()))?;

    info!(
        "Loaded {} named txs from snapshot into DB",
        snapshot.named_txs.len()
    );

    Ok(snapshot)
}
