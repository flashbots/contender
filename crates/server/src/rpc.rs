use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use contender_cli::default_scenarios::BuiltinScenarioCli;
use contender_core::test_scenario::Url;
use contender_testfile::TestConfig;
use jsonrpsee::{proc_macros::rpc, types::ErrorObjectOwned};
use serde::Deserialize;
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
    /// Base64-encoded TOML test config. If omitted, the default uniV2 scenario is used.
    pub test_config_toml_b64: Option<String>,
    /// JSON-encoded test config. If both this and the base64 version are provided, this takes precedence.
    pub test_config: Option<TestConfig>,
    // TODO: support builtin scenarios
    pub test_config_builtin: Option<BuiltinScenarioCli>,
}

impl AddSessionParams {
    fn decode_test_config_toml_b64(&self) -> Result<TestConfig, ContenderRpcError> {
        if let Some(b64) = &self.test_config_toml_b64 {
            let bytes = BASE64.decode(b64)?;
            debug!(
                "Decoded test config from base64, length {} bytes",
                bytes.len()
            );
            let config_str = String::from_utf8(bytes).map_err(ContenderRpcError::InvalidUtf8)?;
            TestConfig::from_str(&config_str).map_err(ContenderRpcError::InvalidTestConfig)
        } else {
            Ok(
                TestConfig::from_str(include_str!("../../../scenarios/uniV2.toml"))
                    .expect("default config should be valid"),
            )
        }
    }

    fn decode_test_config_builtin(&self) -> Result<TestConfig, ContenderRpcError> {
        if let Some(builtin) = &self.test_config_builtin {
            // builtin.to_builtin_scenario(provider, spam_args, data_dir)
            todo!()
        } else {
            Ok(
                TestConfig::from_str(include_str!("../../../scenarios/uniV2.toml"))
                    .expect("default config should be valid"),
            )
        }
    }

    pub fn to_new_session_params(self) -> Result<NewSessionParams, ContenderRpcError> {
        if self.test_config.is_some() && self.test_config_toml_b64.is_some() {
            debug!("Both test_config and test_config_b64 provided, returning error");
            return Err(ContenderRpcError::InvalidArguments(
                "Cannot provide both test_config and test_config_b64".into(),
            ));
        }
        let test_config = if let Some(config) = self.test_config {
            config
        } else {
            self.decode_test_config_toml_b64()?
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

        let session = sessions.add_session(params.to_new_session_params()?);
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
