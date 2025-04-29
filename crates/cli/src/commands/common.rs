//! This file contains type definition for CLI arguments.

use crate::util::{EngineParams, TxTypeCli};

use super::EngineArgs;

#[derive(Debug, clap::Args)]
pub struct ScenarioSendTxsCliArgs {
    /// The path to the test file to use for spamming.
    pub testfile: String,

    /// The HTTP JSON-RPC URL to spam with requests.
    #[arg(
        short,
        long,
        long_help = "RPC URL to test the scenario.",
        default_value = "http://localhost:8545"
    )]
    pub rpc_url: String,

    /// The seed to use for generating spam transactions & accounts.
    #[arg(
        short,
        long,
        long_help = "The seed to use for generating spam transactions"
    )]
    pub seed: Option<String>,

    /// The private keys to use for blockwise spamming.
    /// Required if `txs_per_block` is set.
    #[arg(
        short,
        long = "priv-key",
        long_help = "Add private keys to wallet map. Used to fund agent accounts or sign transactions.
May be specified multiple times."
    )]
    pub private_keys: Option<Vec<String>>,

    /// The minimum balance to check for each private key.
    #[arg(
        long,
        long_help = "The minimum balance to check for each private key in decimal-ETH format (`--min-balance 1.5` means 1.5 * 1e18 wei).",
        default_value = "0.01"
    )]
    pub min_balance: String,

    /// Transaction type
    #[arg(
            short = 't',
            long,
            long_help = "Transaction type for generated transactions.",
            value_enum,
            default_value_t = TxTypeCli::Eip1559,
        )]
    pub tx_type: TxTypeCli,

    #[command(flatten)]
    pub auth_args: AuthCliArgs,
}

#[derive(Debug, clap::Args)]
pub struct AuthCliArgs {
    /// Auth RPC URL for `engine_` calls
    #[arg(
        long,
        long_help = "Provide this URL to enable use of engine_ calls.",
        visible_aliases = &["auth"]
    )]
    pub auth_rpc_url: Option<String>,

    /// JWT secret used for `engine_` calls
    #[arg(
        long,
        long_help = "JWT secret.
Required if --auth-rpc-url is set.",
        visible_aliases = &["jwt"]
    )]
    pub jwt_secret: Option<String>,

    /// Call `engine_forkchoiceUpdated` after each block
    #[arg(
        long,
        long_help = "Call engine_forkchoiceUpdated on Auth RPC after each block.
Requires --auth-rpc-url and --jwt-secret to be set.",
        visible_aliases = &["fcu"]
    )]
    pub call_forkchoice: bool,

    /// Use OP engine provider
    #[arg(
        long = "optimism",
        long_help = "Set this flag when targeting an OP node.",
        visible_aliases = &["op"]
    )]
    pub use_op: bool,
}

impl AuthCliArgs {
    pub async fn engine_params(&self) -> Result<EngineParams, Box<dyn std::error::Error>> {
        if self.call_forkchoice && (self.auth_rpc_url.is_none() || self.jwt_secret.is_none()) {
            return Err("engine args required for forkchoice".into());
        }

        let engine_params = if self.auth_rpc_url.is_some() && self.jwt_secret.is_some() {
            let args = EngineArgs {
                auth_rpc_url: self.auth_rpc_url.to_owned().expect("auth_rpc_url"),
                jwt_secret: self.jwt_secret.to_owned().expect("jwt_secret").into(),
                use_op: self.use_op,
            };
            EngineParams::new(args.new_provider().await?, self.call_forkchoice)
        } else {
            EngineParams::default()
        };
        Ok(engine_params)
    }
}

#[derive(Debug, clap::Args)]
pub struct SendSpamCliArgs {
    /// HTTP JSON-RPC URL to use for bundle spamming (must support `eth_sendBundle`).
    #[arg(
        short,
        long,
        long_help = "HTTP JSON-RPC URL to use for bundle spamming (must support `eth_sendBundle`)"
    )]
    pub builder_url: Option<String>,

    /// The number of txs to send per second using the timed spammer. This is the default spammer.
    /// May not be set if `txs_per_block` is set.
    #[arg(long, long_help = "Number of txs to send per second. Must not be set if --txs-per-block is set.", visible_aliases = &["tps"])]
    pub txs_per_second: Option<u64>,

    /// The number of txs to send per block using the blockwise spammer.
    /// May not be set if `txs_per_second` is set. Requires `prv_keys` to be set.
    #[arg(
            long,
            long_help =
"Number of txs to send per block. Must not be set if --txs-per-second is set.
Requires --priv-key to be set for each 'from' address in the given testfile.",
        visible_aliases = &["tpb"])]
    pub txs_per_block: Option<u64>,

    /// The duration of the spamming run in seconds or blocks, depending on whether `txs_per_second` or `txs_per_block` is set.
    #[arg(
        short,
        long,
        default_value = "1",
        long_help = "Duration of the spamming run in seconds or blocks, depending on whether --txs-per-second or --txs-per-block is set."
    )]
    pub duration: u64,

    /// The time to wait for pending transactions to land, in seconds.
    #[arg(
        short = 'w',
        long,
        default_value = "12",
        long_help = "The number of seconds to wait for pending transactions to land.",
        visible_aliases = &["wait"]
    )]
    pub timeout: u64,
}
