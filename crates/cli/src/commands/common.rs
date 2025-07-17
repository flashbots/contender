//! This file contains type definition for CLI arguments.

use super::EngineArgs;
use alloy::consensus::TxType;
use alloy::primitives::utils::parse_units;
use alloy::primitives::U256;
use contender_core::BundleType;
use contender_engine_provider::reth_node_api::EngineApiMessageVersion;
use contender_engine_provider::AdvanceChain;
use std::sync::Arc;

#[derive(Clone, Debug, clap::Args)]
pub struct ScenarioSendTxsCliArgs {
    /// The path to the test file to use for spamming/setup. Use default scenarios with "scenario:<filename>".
    /// Default scenarios can be found at https://github.com/flashbots/contender/tree/main/scenarios
    /// Example: `scenario:simple.toml` or `scenario:precompiles/modexp.toml`
    pub testfile: Option<String>,

    /// RPC URL to send requests.
    #[arg(
        short,
        long,
        long_help = "RPC URL to send requests from the `eth_` namespace. Set --builder-url or --auth-rpc-url to enable other namespaces.",
        default_value = "http://localhost:8545"
    )]
    pub rpc_url: String,

    /// The seed to use for generating spam transactions.
    #[arg(
        short,
        long,
        long_help = "The seed to use for generating spam transactions"
    )]
    pub seed: Option<String>,

    /// Private key(s) to use for funding agent accounts or signing transactions.
    #[arg(
        short,
        long = "priv-key",
        long_help = "Add private keys to fund agent accounts. Scenarios with hard-coded `from` addresses may also use these to sign transactions.
May be specified multiple times."
    )]
    pub private_keys: Option<Vec<String>>,

    /// The minimum balance to keep in each spammer EOA.
    #[arg(
        long,
        long_help = "The minimum balance to keep in each spammer EOA, with units.",
        default_value = "0.01 ether",
        value_parser = parse_amount,
    )]
    pub min_balance: U256,

    /// Transaction type
    #[arg(
            short = 't',
            long,
            long_help = "Transaction type for generated transactions.",
            value_enum,
            default_value_t = TxTypeCli::Eip1559,
        )]
    pub tx_type: TxTypeCli,

    /// Bundle type
    #[arg(
        long,
        long_help = "Bundle type for generated bundles.",
        value_enum,
        default_value_t = BundleTypeCli::default(),
        visible_aliases = &["bt"]
    )]
    pub bundle_type: BundleTypeCli,

    #[command(flatten)]
    pub auth_args: AuthCliArgs,

    #[arg(
        short,
        long,
        value_name="KEY=VALUE",
        long_help = "Key-value pairs to override the parameters in scenario files.",
        value_parser = cli_env_vars_parser,
        action = clap::ArgAction::Append,
    )]
    pub env: Option<Vec<(String, String)>>,
}

#[derive(Clone, Debug, clap::Args)]
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
        long_help = "Use OP types in the engine provider. Set this flag when targeting an OP node.",
        visible_aliases = &["op"]
    )]
    pub use_op: bool,

    /// Engine API Message Version
    #[arg(
        long,
        short,
        value_enum,
        default_value_t = EngineMessageVersion::V4
    )]
    message_version: EngineMessageVersion,
}

#[derive(Copy, Debug, Clone, clap::ValueEnum)]
enum EngineMessageVersion {
    V1,
    V2,
    V3,
    V4,
    // V5,
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
                message_version: match self.message_version {
                    EngineMessageVersion::V1 => EngineApiMessageVersion::V1,
                    EngineMessageVersion::V2 => EngineApiMessageVersion::V2,
                    EngineMessageVersion::V3 => EngineApiMessageVersion::V3,
                    EngineMessageVersion::V4 => EngineApiMessageVersion::V4,
                    // EngineMessageVersion::V5 => EngineApiMessageVersion::V5,
                },
            };
            EngineParams::new(Arc::new(args.new_provider().await?), self.call_forkchoice)
        } else {
            EngineParams::default()
        };
        Ok(engine_params)
    }
}

#[derive(Clone, Debug, clap::Args)]
pub struct SendSpamCliArgs {
    /// HTTP JSON-RPC URL to use for bundle spamming (must support `eth_sendBundle`).
    #[arg(
        short,
        long,
        long_help = "HTTP JSON-RPC URL to use for bundle spamming (must support `eth_sendBundle`)"
    )]
    pub builder_url: Option<String>,

    /// The number of txs to send per second using the timed spammer.
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
        default_value_t = 1,
        long_help = "Duration of the spamming run in seconds or blocks, depending on whether --txs-per-second or --txs-per-block is set."
    )]
    pub duration: u64, // TODO: make a new enum to represent seconds or blocks

    /// The time to wait for pending transactions to land, in blocks.
    #[arg(
        short = 'w',
        long,
        default_value_t = 12,
        long_help = "The number of blocks to wait for pending transactions to land. If transactions land within the timeout, it resets.",
        visible_aliases = &["wait"]
    )]
    pub timeout: u64,

    /// The number of times to repeat the spam run.
    /// If set with a value, the spam run will be repeated this many times.
    /// If set without a value, the spam run will be repeated indefinitely.
    /// If not set, the spam run will be executed once.
    #[arg(
        short,
        long,
        num_args = 0..=1,
        long_help = "The number of times to repeat the spam run. If set with a value, the spam run will be repeated this many times. If set without a value, the spam run will be repeated indefinitely. If not set, the spam run will be repeated once."
    )]
    pub loops: Option<Option<u64>>,

    /// The number of accounts to generate for each agent (`from_pool` in scenario files)
    #[arg(
        short,
        long,
        visible_aliases = &["na", "accounts"],
        default_value_t = 10
    )]
    pub accounts_per_agent: u64,
}

#[derive(Copy, Debug, Clone, clap::ValueEnum)]
pub enum TxTypeCli {
    /// Legacy transaction (type `0x0`)
    Legacy,
    // /// Transaction with an [`AccessList`] ([EIP-2930](https://eips.ethereum.org/EIPS/eip-2930)), type `0x1`
    // Eip2930,
    /// A transaction with a priority fee ([EIP-1559](https://eips.ethereum.org/EIPS/eip-1559)), type `0x2`
    Eip1559,
    /// Shard Blob Transactions ([EIP-4844](https://eips.ethereum.org/EIPS/eip-4844)), type `0x3`
    Eip4844,
    // /// EOA Set Code Transactions ([EIP-7702](https://eips.ethereum.org/EIPS/eip-7702)), type `0x4`
    // Eip7702,
}

#[derive(Copy, Debug, Clone, clap::ValueEnum)]
pub enum BundleTypeCli {
    L1,
    #[clap(name = "no-revert")]
    RevertProtected,
}

impl Default for BundleTypeCli {
    fn default() -> Self {
        BundleType::default().into()
    }
}

pub struct EngineParams {
    pub engine_provider: Option<Arc<dyn AdvanceChain + Send + Sync + 'static>>,
    pub call_fcu: bool,
}

impl EngineParams {
    pub fn new(
        engine_provider: Arc<dyn AdvanceChain + Send + Sync + 'static>,
        call_forkchoice: bool,
    ) -> Self {
        Self {
            engine_provider: Some(engine_provider),
            call_fcu: call_forkchoice,
        }
    }
}

/// default is Eth wrapper with no provider
impl Default for EngineParams {
    fn default() -> Self {
        Self {
            engine_provider: None,
            call_fcu: false,
        }
    }
}

impl From<BundleType> for BundleTypeCli {
    fn from(value: BundleType) -> Self {
        match value {
            BundleType::L1 => BundleTypeCli::L1,
            BundleType::RevertProtected => BundleTypeCli::RevertProtected,
        }
    }
}

impl From<BundleTypeCli> for BundleType {
    fn from(value: BundleTypeCli) -> Self {
        match value {
            BundleTypeCli::L1 => BundleType::L1,
            BundleTypeCli::RevertProtected => BundleType::RevertProtected,
        }
    }
}

impl From<TxTypeCli> for alloy::consensus::TxType {
    fn from(value: TxTypeCli) -> Self {
        match value {
            TxTypeCli::Legacy => TxType::Legacy,
            // TxTypeCli::Eip2930 => TxType::Eip2930,
            TxTypeCli::Eip1559 => TxType::Eip1559,
            TxTypeCli::Eip4844 => TxType::Eip4844,
            // TxTypeCli::Eip7702 => TxType::Eip7702,
        }
    }
}

pub fn cli_env_vars_parser(s: &str) -> Result<(String, String), String> {
    let equal_sign_index = s.find('=').ok_or("Invalid KEY=VALUE: No \"=\" found")?;

    if equal_sign_index == 0 {
        return Err("Empty Key: No Key found".to_owned());
    }

    Ok((
        s[..equal_sign_index].to_string(),
        s[equal_sign_index + 1..].to_string(),
    ))
}

/// Parses an amount string with units (e.g., "1 ether", "100 gwei") into a U256 value.
/// Used for inline parsing of amounts in CLI arguments.
pub fn parse_amount(input: &str) -> Result<U256, String> {
    let input = input.trim().to_lowercase();
    let (num_str, unit) = input.trim().split_at(
        input
            .find(|c: char| !c.is_numeric() && c != '.')
            .ok_or("Missing unit in amount")?,
    );
    let unit = unit.trim();
    let value: U256 = parse_units(num_str, unit)
        .map_err(|e| format!("Failed to parse units: {e}"))?
        .into();

    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn correct_parsing_env_vars() {
        let env_param_value = "key1=value1";
        assert_eq!(
            cli_env_vars_parser(env_param_value).unwrap(),
            ("key1".to_owned(), "value1".to_owned())
        );
    }

    #[test]
    fn multiple_equal_signs() {
        let env_param_value = "key1=value1==";
        assert_eq!(
            cli_env_vars_parser(env_param_value).unwrap(),
            ("key1".to_owned(), "value1==".to_owned())
        );
    }

    #[test]
    fn empty_env() {
        let env_param_value = "";
        assert_eq!(
            cli_env_vars_parser(env_param_value)
                .err()
                .unwrap()
                .to_string(),
            "Invalid KEY=VALUE: No \"=\" found"
        )
    }

    #[test]
    fn empty_key() {
        let env_param_value = "=value1";
        assert_eq!(
            cli_env_vars_parser(env_param_value)
                .err()
                .unwrap()
                .to_string(),
            "Empty Key: No Key found"
        )
    }
}
