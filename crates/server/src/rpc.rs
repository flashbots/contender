use contender_core::test_scenario::Url;
use jsonrpsee::{
    proc_macros::rpc,
    types::{ErrorObject, ErrorObjectOwned},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::info;

use crate::sessions::{ContenderSessionCache, ContenderSessionInfo};

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

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AddSessionParams {
    pub name: String,
    pub rpc_url: Url,
}

#[derive(Debug, Error)]
enum ContenderRpcError {
    #[error("Failed to initialize contender session: {0}")]
    SessionInitializationFailed(contender_core::Error),
}

impl From<ContenderRpcError> for ErrorObjectOwned {
    fn from(err: ContenderRpcError) -> Self {
        match err {
            ContenderRpcError::SessionInitializationFailed(e) => ErrorObject::owned(
                1,
                "Failed to initialize contender session".to_string(),
                Some(e.to_string()),
            ),
        }
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
        let session = sessions.add_session(params);
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
