//! This file contains type definition for CLI arguments.

use super::EngineArgs;
use crate::commands::error::ArgsError;
use crate::commands::SpamScenario;
use crate::error::CliError;
use crate::util::get_signers_with_defaults;
use alloy::consensus::TxType;
use alloy::primitives::U256;
use alloy::providers::{DynProvider, ProviderBuilder};
use alloy::signers::local::PrivateKeySigner;
use contender_core::generator::util::parse_value;
use contender_core::test_scenario::Url;
use contender_core::BundleType;
use contender_engine_provider::reth_node_api::EngineApiMessageVersion;
use contender_engine_provider::ControlChain;
use contender_testfile::TestConfig;
use op_alloy_network::AnyNetwork;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use tracing::info;

#[derive(Clone, Debug, clap::Args)]
pub struct ScenarioSendTxsCliArgs {
    /// The path to the test file to use for spamming/setup. Use default scenarios with "scenario:<filename>".
    /// Default scenarios can be found at https://github.com/flashbots/contender/tree/main/scenarios
    /// Example: `scenario:simple.toml` or `scenario:precompiles/modexp.toml`
    pub testfile: Option<String>,

    #[command(flatten)]
    pub rpc_args: SendTxsCliArgsInner,
}

#[derive(Clone, Debug, clap::Args)]
pub struct SendTxsCliArgsInner {
    /// RPC URL to send requests.
    #[arg(
        env = "RPC_URL",
        short,
        long,
        long_help = "RPC URL to send requests from the `eth_` namespace. Set --builder-url or --auth-rpc-url to enable other namespaces.",
        default_value = "http://localhost:8545",
        visible_aliases = ["el-rpc", "el-rpc-url"]
    )]
    pub rpc_url: Url,

    /// The seed to use for generating spam transactions.
    #[arg(
        env = "CONTENDER_SEED",
        short,
        long,
        long_help = "The seed to use for generating spam transactions"
    )]
    pub seed: Option<String>,

    /// Private key(s) to use for funding agent accounts or signing transactions.
    #[arg(
        env = "CONTENDER_PRIVATE_KEY",
        short,
        long = "priv-key",
        long_help = "Add private keys to fund agent accounts. Scenarios with hard-coded `from` addresses may also use these to sign transactions.
Flag may be specified multiple times."
    )]
    pub private_keys: Option<Vec<String>>,

    /// The minimum balance to keep in each spammer EOA.
    #[arg(
        long,
        long_help = "The minimum balance to keep in each spammer EOA, with units.",
        default_value = "0.01 ether",
        value_parser = parse_value,
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
        visible_aliases = ["bt"]
    )]
    pub bundle_type: BundleTypeCli,

    #[command(flatten)]
    pub auth_args: AuthCliArgs,

    /// Enable block-building.
    #[arg(
        long,
        long_help = "Enable block-building by calling engine_forkchoiceUpdated on Auth RPC after each spam batch.
Requires --auth-rpc-url and --jwt-secret to be set.",
        visible_aliases = ["fcu", "build-blocks"]
    )]
    pub call_forkchoice: bool,

    #[arg(
        short,
        long,
        value_name="KEY=VALUE",
        long_help = "Key-value pairs to override the parameters in scenario files.",
        value_parser = cli_env_vars_parser,
        action = clap::ArgAction::Append,
    )]
    pub env: Option<Vec<(String, String)>>,

    #[arg(
        long,
        long_help = "Override senders to send all transactions from one account."
    )]
    pub override_senders: bool,

    /// The gas price to use for the spammer.
    #[arg(
        long,
        long_help = "The gas price to use for the spammer, with units, defaults to Wei.",
        value_parser = parse_value,
    )]
    pub gas_price: Option<U256>,

    /// The number of accounts to generate for each agent (`from_pool` in scenario files).
    /// Defaults to 1 for standalone setup, 10 for spam and campaign.
    #[arg(
        short = 'a',
        long,
        visible_aliases = ["na", "accounts"],
    )]
    pub accounts_per_agent: Option<u64>,
}

impl SendTxsCliArgsInner {
    /// Returns the accounts_per_agent value, or the provided default if not set.
    pub fn accounts_per_agent_or(&self, default: u64) -> u64 {
        self.accounts_per_agent.unwrap_or(default)
    }

    pub fn new_rpc_provider(&self) -> Result<DynProvider<AnyNetwork>, ArgsError> {
        info!("connecting to {}", self.rpc_url);
        Ok(DynProvider::new(
            ProviderBuilder::new()
                .network::<AnyNetwork>()
                .connect_http(self.rpc_url.clone()),
        ))
    }

    pub fn user_signers(&self) -> Vec<PrivateKeySigner> {
        self.private_keys
            .as_ref()
            .unwrap_or(&vec![])
            .iter()
            .map(|key| PrivateKeySigner::from_str(key).expect("invalid private key"))
            .collect::<Vec<PrivateKeySigner>>()
    }

    pub fn user_signers_with_defaults(&self) -> Vec<PrivateKeySigner> {
        get_signers_with_defaults(self.private_keys.to_owned())
    }

    /// This account is used to fund agent accounts, and is used as the signer when `--override-senders` is passed.
    pub fn primary_signer(&self) -> PrivateKeySigner {
        self.user_signers_with_defaults()[0].to_owned()
    }

    pub async fn testconfig(&self, scenario: &SpamScenario) -> Result<TestConfig, CliError> {
        let mut testconfig = scenario.testconfig().await?;
        if self.override_senders {
            testconfig.override_senders(self.primary_signer().address());
        }
        Ok(testconfig)
    }
}

#[derive(Clone, Debug, clap::Args)]
pub struct AuthCliArgs {
    /// Auth RPC URL for `engine_` calls
    #[arg(
        env = "AUTH_RPC_URL",
        long,
        long_help = "Provide this URL to enable use of engine_ calls.",
        visible_aliases = ["auth", "auth-rpc", "auth-url"]
    )]
    pub auth_rpc_url: Option<Url>,

    /// Path to file containing JWT secret
    #[arg(
        env = "JWT_SECRET_PATH",
        long,
        long_help = "Path to file containing JWT secret used for `engine_` calls.
Required if --auth-rpc-url is set.",
        visible_aliases = ["jwt"]
    )]
    pub jwt_secret: Option<PathBuf>,

    /// Use OP engine provider
    #[arg(
        long = "optimism",
        long_help = "Use OP types in the engine provider. Set this flag when targeting an OP node.",
        visible_aliases = ["op"]
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
    pub async fn engine_params(&self, call_forkchoice: bool) -> Result<EngineParams, CliError> {
        if call_forkchoice && (self.auth_rpc_url.is_none() || self.jwt_secret.is_none()) {
            return Err(ArgsError::engine_args_required(
                self.auth_rpc_url.clone(),
                self.jwt_secret.clone(),
            )
            .into());
        }

        let engine_params = if self.auth_rpc_url.is_some() && self.jwt_secret.is_some() {
            let args = EngineArgs {
                auth_rpc_url: self.auth_rpc_url.to_owned().expect("auth_rpc_url"),
                jwt_secret: self.jwt_secret.to_owned().expect("jwt_secret"),
                use_op: self.use_op,
                message_version: match self.message_version {
                    EngineMessageVersion::V1 => EngineApiMessageVersion::V1,
                    EngineMessageVersion::V2 => EngineApiMessageVersion::V2,
                    EngineMessageVersion::V3 => EngineApiMessageVersion::V3,
                    EngineMessageVersion::V4 => EngineApiMessageVersion::V4,
                    // EngineMessageVersion::V5 => EngineApiMessageVersion::V5,
                },
            };
            EngineParams::new(Arc::new(args.new_provider().await?), call_forkchoice)
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
        env = "BUILDER_RPC_URL",
        short,
        long,
        long_help = "HTTP JSON-RPC URL to use for bundle spamming (must support `eth_sendBundle`)",
        visible_aliases = ["builder", "builder-rpc-url", "builder-rpc"]
    )]
    pub builder_url: Option<Url>,

    /// The number of txs to send per second using the timed spammer.
    /// May not be set if `txs_per_block` is set.
    #[arg(global = true, long, long_help = "Number of txs to send per second. Must not be set if --txs-per-block is set.", visible_aliases = ["tps"])]
    pub txs_per_second: Option<u64>,

    /// The number of txs to send per block using the blockwise spammer.
    /// May not be set if `txs_per_second` is set. Requires `prv_keys` to be set.
    #[arg(
        global = true,
            long,
            long_help =
"Number of txs to send per block. Must not be set if --txs-per-second is set.
Requires --priv-key to be set for each 'from' address in the given testfile.",
        visible_aliases = ["tpb"])]
    pub txs_per_block: Option<u64>,

    /// The duration of the spamming run in seconds or blocks, depending on whether `txs_per_second` or `txs_per_block` is set.
    #[arg(
        short,
        long,
        default_value_t = 10,
        long_help = "Duration of the spamming run in seconds or blocks, depending on whether --txs-per-second or --txs-per-block is set."
    )]
    pub duration: u64, // TODO: make a new enum to represent seconds or blocks

    /// The time to wait for pending transactions to land, in blocks.
    #[arg(
        short = 'w',
        long,
        default_value_t = 12,
        long_help = "The number of blocks to wait for pending transactions to land. If transactions land within the timeout, it resets.",
        visible_aliases = ["wait"]
    )]
    pub pending_timeout: u64,

    /// Run spammer indefinitely.
    #[arg(
        global = true,
        default_value_t = false,
        long = "forever",
        visible_aliases = ["indefinite", "indefinitely", "infinite"]
    )]
    pub run_forever: bool,
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
    /// EOA Set Code Transactions ([EIP-7702](https://eips.ethereum.org/EIPS/eip-7702)), type `0x4`
    Eip7702,
}

impl std::fmt::Display for TxTypeCli {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use TxTypeCli::*;
        write!(
            f,
            "{}",
            match self {
                Legacy => "legacy",
                Eip1559 => "eip1559",
                Eip4844 => "eip4844",
                Eip7702 => "eip7702",
            }
        )
    }
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

#[derive(Clone)]
pub struct EngineParams {
    pub engine_provider: Option<Arc<dyn ControlChain + Send + Sync + 'static>>,
    pub call_fcu: bool,
}

impl EngineParams {
    pub fn new(
        engine_provider: Arc<dyn ControlChain + Send + Sync + 'static>,
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
            TxTypeCli::Eip7702 => TxType::Eip7702,
        }
    }
}

impl PartialEq<alloy::consensus::TxType> for TxTypeCli {
    fn eq(&self, other: &alloy::consensus::TxType) -> bool {
        matches!(
            (self, other),
            (TxTypeCli::Legacy, alloy::consensus::TxType::Legacy)
                | (TxTypeCli::Eip1559, alloy::consensus::TxType::Eip1559)
                | (TxTypeCli::Eip4844, alloy::consensus::TxType::Eip4844)
                | (TxTypeCli::Eip7702, alloy::consensus::TxType::Eip7702)
        )
    }
}

impl PartialEq<TxTypeCli> for alloy::consensus::TxType {
    fn eq(&self, other: &TxTypeCli) -> bool {
        other == self
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
