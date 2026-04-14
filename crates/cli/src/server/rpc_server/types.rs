use crate::server::{error::ContenderRpcError, sessions::NewSessionParams};
use crate::{
    commands::common::{BundleTypeCli, EngineMessageVersion, TxTypeCli},
    default_scenarios::{BuiltinOptions, BuiltinScenarioCli},
    util::provider::AuthClient,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use contender_core::{
    agent_controller::AgentClass,
    alloy::{
        network::{AnyNetwork, Ethereum},
        primitives::{B256, U256},
        providers::{DynProvider, ProviderBuilder},
        rpc::types::engine::JwtSecret,
    },
    generator::{agent_pools::AgentSpec, RandSeed},
    test_scenario::Url,
    RunOpts,
};
use contender_engine_provider::{AuthProvider, ControlChain};
use contender_testfile::TestConfig;
use op_alloy_network::Optimism;
use serde::{Deserialize, Deserializer, Serialize};
use std::{collections::HashMap, str::FromStr, time::Duration};
use tracing::debug;

/// Data returned from the `status` endpoint, containing general info about the server.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerStatus {
    pub num_sessions: usize,
}

/// RPC parameters for adding a new contender session.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddSessionParams {
    pub name: String,
    pub rpc_url: Url,
    pub test_config: Option<TestConfigSource>,
    pub options: Option<SessionOptions>,
}

impl AddSessionParams {
    pub async fn to_new_session_params(
        self,
        seed: RandSeed,
    ) -> Result<NewSessionParams, ContenderRpcError> {
        let test_config = if let Some(config) = self.test_config {
            let provider = DynProvider::new(
                ProviderBuilder::new()
                    .network::<AnyNetwork>()
                    .connect_http(self.rpc_url.clone()),
            );
            config
                .to_testconfig(
                    Some(BuiltinOptions {
                        accounts_per_agent: None,
                        seed,
                        spam_rate: None,
                    }),
                    &provider,
                )
                .await?
        } else {
            TestConfig::from_str(include_str!("../../../../../scenarios/uniV2.toml"))
                .expect("default config should be valid")
        };

        Ok(NewSessionParams {
            name: self.name.clone(),
            rpc_url: self.rpc_url.clone(),
            test_config,
            options: self.options.unwrap_or_default(),
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum TestConfigSource {
    TomlBase64(String),
    Json(TestConfig),
    Builtin(BuiltinScenarioCli),
}

impl TestConfigSource {
    pub async fn to_testconfig(
        self,
        builtin_options: Option<BuiltinOptions>,
        provider: &DynProvider<AnyNetwork>,
    ) -> Result<TestConfig, ContenderRpcError> {
        match self {
            TestConfigSource::TomlBase64(b64) => {
                let bytes = BASE64.decode(b64)?;
                debug!(
                    "Decoded test config from base64, length {} bytes",
                    bytes.len()
                );
                let config_str =
                    String::from_utf8(bytes).map_err(ContenderRpcError::InvalidUtf8)?;
                TestConfig::from_str(&config_str).map_err(ContenderRpcError::InvalidTestConfig)
            }

            TestConfigSource::Json(config) => Ok(config),

            TestConfigSource::Builtin(builtin) => {
                let scenario = builtin
                    .to_builtin_scenario(provider, builtin_options.unwrap_or_default())
                    .await
                    .unwrap()
                    .into();
                Ok(scenario)
            }
        }
    }
}

/// RPC parameters for the `spam` method.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SpamParams {
    pub session_id: usize,
    /// Number of transactions per period. Defaults to 10.
    pub txs_per_period: Option<u64>,
    /// Number of periods (seconds or blocks). Defaults to 10.
    pub duration: Option<u64>,
    /// Which spammer to use. Defaults to `Timed`.
    pub spammer: Option<SpammerType>,
    /// Human-readable name for this spam run.
    pub name: Option<String>,
    /// Whether to look for receipts while spamming; enables onchain metrics collection.
    pub save_receipts: Option<bool>,
    /// When true, the spammer restarts automatically after each run until stopped.
    pub run_forever: Option<bool>,
    /// When passed, the server will log a summary of the current spam run every `reportIntervalSecs` seconds. Note that this may have a performance impact if set to a very low value.
    pub report_interval_secs: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SpammerType {
    /// Send a batch of txs at a fixed time interval (1 second).
    #[default]
    Timed,
    /// Send a batch of txs every new block.
    Blockwise,
}

impl SpamParams {
    pub fn as_run_opts(&self) -> RunOpts {
        let mut opts = RunOpts::new();
        if let Some(n) = self.txs_per_period {
            opts = opts.txs_per_period(n);
        }
        if let Some(n) = self.duration {
            opts = opts.periods(n);
        }
        if let Some(name) = &self.name {
            opts = opts.name(name);
        }
        if let Some(report_interval) = self.report_interval_secs {
            opts = opts.report_interval_secs(report_interval);
        }
        opts
    }
}

#[derive(Clone, Debug)]
pub struct JwtParam {
    secret: JwtSecret,
}

impl<'a> serde::Deserialize<'a> for JwtParam {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'a>,
    {
        let s = String::deserialize(deserializer)?;
        debug!("deserialized string for JwtSecret: {s}"); // TODO: delete this line
        Ok(JwtParam {
            secret: JwtSecret::from_str(&s).map_err(serde::de::Error::custom)?,
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthParams {
    pub jwt_secret: JwtParam,
    pub message_version: EngineMessageVersion,
    pub rpc_url: Url,
    pub call_fcu: Option<bool>,
    pub use_op: Option<bool>,
}

impl AuthParams {
    pub async fn new_provider(&self) -> Result<AuthClient, ContenderRpcError> {
        let provider: Box<dyn ControlChain + Send + Sync + 'static> =
            if self.use_op.unwrap_or(false) {
                Box::new(
                    AuthProvider::<Optimism>::new(
                        &self.rpc_url,
                        self.jwt_secret.secret.clone(),
                        self.message_version.into(),
                    )
                    .await?,
                )
            } else {
                Box::new(
                    AuthProvider::<Ethereum>::new(
                        &self.rpc_url,
                        self.jwt_secret.secret,
                        self.message_version.into(),
                    )
                    .await?,
                )
            };
        Ok(AuthClient::new(provider))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuilderParams {
    pub rpc_url: Url,
    pub bundle_type: BundleTypeCli,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionOptions {
    pub auth: Option<AuthParams>,
    pub builder: Option<BuilderParams>,
    pub min_balance: Option<U256>,
    #[serde(rename = "timeoutSecs")]
    pub pending_tx_timeout: Option<Duration>,
    pub tx_type: Option<TxTypeCli>,
    pub private_keys: Option<Vec<B256>>,
    pub agents: Option<AgentParams>,
    pub env: Option<HashMap<String, String>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentParams {
    pub create_accounts: Option<usize>,
    pub setup_accounts: Option<usize>,
    pub spam_accounts: Option<usize>,
}

impl From<AgentParams> for AgentSpec {
    fn from(params: AgentParams) -> Self {
        let mut spec = AgentSpec::default();
        if let Some(n) = params.create_accounts {
            spec = spec.create_accounts(n);
        }
        if let Some(n) = params.setup_accounts {
            spec = spec.setup_accounts(n);
        }
        if let Some(n) = params.spam_accounts {
            spec = spec.spam_accounts(n);
        }
        spec
    }
}

fn parse_value<'de, D: Deserializer<'de>>(deserializer: D) -> Result<U256, D::Error> {
    let value = String::deserialize(deserializer)?;
    contender_core::generator::util::parse_value(&value)
        .map_err(|e| serde::de::Error::custom(format!("failed to parse value '{value}': {e}")))
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FundAccountsParams {
    pub session_id: usize,
    pub agent_class: Option<AgentClass>,
    #[serde(deserialize_with = "parse_value")]
    pub amount: U256,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::default_scenarios::transfers::TransferStressCliArgs;
    use base64::engine::general_purpose::STANDARD as BASE64;
    use contender_core::alloy::{
        consensus::constants::ETH_TO_WEI,
        primitives::{Address, U256},
    };

    #[test]
    fn test_toml_base64_variant() {
        let toml_content = include_str!("../../../../../scenarios/uniV2.toml");
        let b64 = BASE64.encode(toml_content);
        let json = serde_json::json!({ "TomlBase64": b64 });
        // println!(
        //     "TomlBase64:\n{}\n",
        //     serde_json::to_string_pretty(&json).unwrap()
        // );

        let source: TestConfigSource = serde_json::from_value(json).unwrap();
        assert!(matches!(source, TestConfigSource::TomlBase64(_)));
    }

    #[test]
    fn test_json_variant() {
        let config =
            TestConfig::from_str(include_str!("../../../../../scenarios/uniV2.toml")).unwrap();
        let json = serde_json::json!({ "Json": config });
        // println!("Json:\n{}\n", serde_json::to_string_pretty(&json).unwrap());

        let source: TestConfigSource = serde_json::from_value(json).unwrap();
        assert!(matches!(source, TestConfigSource::Json(_)));
    }

    #[tokio::test]
    async fn test_builtin_variant() {
        let builtin =
            TestConfigSource::Builtin(BuiltinScenarioCli::Transfers(TransferStressCliArgs {
                amount: U256::from(ETH_TO_WEI),
                recipient: Some(Address::ZERO),
            }));
        let json = serde_json::json!(builtin);
        // println!("{}", serde_json::to_string_pretty(&json).unwrap());

        let source: TestConfigSource = serde_json::from_value(json).unwrap();
        assert!(matches!(source, TestConfigSource::Builtin(_)));
    }
}
