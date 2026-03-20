use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use contender_cli::default_scenarios::{BuiltinOptions, BuiltinScenarioCli};
use contender_core::{
    alloy::{
        network::AnyNetwork,
        providers::{DynProvider, ProviderBuilder},
    },
    generator::RandSeed,
    test_scenario::Url,
};
use contender_testfile::TestConfig;
use jsonrpsee::{proc_macros::rpc, types::ErrorObjectOwned};
use serde::{Deserialize, Serialize};
use std::{str::FromStr, sync::Arc};
use tokio::sync::RwLock;
use tracing::{debug, info};

use crate::{
    error::ContenderRpcError,
    sessions::{ContenderSessionCache, ContenderSessionInfo, NewSessionParams},
};

#[rpc(server)]
pub trait ContenderRpc {
    #[method(name = "status")]
    async fn status(&self) -> jsonrpsee::core::RpcResult<String>;

    #[method(name = "add_session")]
    async fn add_session(
        &self,
        name: AddSessionParams,
    ) -> jsonrpsee::core::RpcResult<ContenderSessionInfo>;

    #[method(name = "get_session")]
    async fn get_session(
        &self,
        id: usize,
    ) -> jsonrpsee::core::RpcResult<Option<ContenderSessionInfo>>;

    #[method(name = "remove_session")]
    async fn remove_session(&self, id: usize) -> jsonrpsee::core::RpcResult<()>;
}

pub struct ContenderServer {
    pub sessions: Arc<RwLock<ContenderSessionCache>>,
}

impl ContenderServer {
    pub fn new(sessions: Arc<RwLock<ContenderSessionCache>>) -> Self {
        Self { sessions }
    }
}

/// RPC parameters for adding a new contender session.
#[derive(Clone, Debug, Deserialize)]
pub struct AddSessionParams {
    pub name: String,
    pub rpc_url: Url,
    pub test_config: Option<TestConfigSource>,
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
            TestConfig::from_str(include_str!("../../../scenarios/uniV2.toml"))
                .expect("default config should be valid")
        };

        Ok(NewSessionParams {
            name: self.name.clone(),
            rpc_url: self.rpc_url.clone(),
            test_config,
        })
    }
}

#[async_trait::async_trait]
impl ContenderRpcServer for ContenderServer {
    async fn status(&self) -> jsonrpsee::core::RpcResult<String> {
        let sessions = self.sessions.read().await;
        Ok(format!("{} session(s) active", sessions.num_sessions()))
    }

    async fn add_session(
        &self,
        params: AddSessionParams,
    ) -> jsonrpsee::core::RpcResult<ContenderSessionInfo> {
        let mut sessions = self.sessions.write().await;

        let session_seed = RandSeed::seed_from_bytes(&sessions.num_sessions().to_be_bytes());
        let session = sessions.add_session(params.to_new_session_params(session_seed).await?);
        let info = session.info.clone();

        info!(
            "Initializing session {} with RPC URL {}",
            info.name, info.rpc_url
        );
        session
            .contender
            .initialize()
            .await
            .map_err(ContenderRpcError::SessionInitializationFailed)
            .map_err(ErrorObjectOwned::from)?;
        info!("Session {} initialized successfully", info.name);
        Ok(info)
    }

    async fn get_session(
        &self,
        id: usize,
    ) -> jsonrpsee::core::RpcResult<Option<ContenderSessionInfo>> {
        let sessions = self.sessions.read().await;
        Ok(sessions.get_session(id).map(|s| s.info.clone()))
    }

    async fn remove_session(&self, id: usize) -> jsonrpsee::core::RpcResult<()> {
        let mut sessions = self.sessions.write().await;
        sessions.remove_session(id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD as BASE64;
    use contender_cli::default_scenarios::transfers::TransferStressCliArgs;
    use contender_core::alloy::{
        consensus::constants::ETH_TO_WEI,
        primitives::{Address, U256},
    };

    #[test]
    fn test_toml_base64_variant() {
        let toml_content = include_str!("../../../scenarios/uniV2.toml");
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
        let config = TestConfig::from_str(include_str!("../../../scenarios/uniV2.toml")).unwrap();
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
        println!("{}", serde_json::to_string_pretty(&json).unwrap());

        let source: TestConfigSource = serde_json::from_value(json).unwrap();
        assert!(matches!(source, TestConfigSource::Builtin(_)));
    }
}
