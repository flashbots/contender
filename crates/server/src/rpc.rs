use std::sync::Arc;

use jsonrpsee::proc_macros::rpc;
use tokio::sync::RwLock;

use crate::sessions::{ContenderSessionCache, ContenderSessionInfo};

#[rpc(server)]
pub trait ContenderRpc {
    #[method(name = "status")]
    async fn status(&self) -> jsonrpsee::core::RpcResult<String>;

    #[method(name = "add_session")]
    async fn add_session(&self, name: String) -> jsonrpsee::core::RpcResult<ContenderSessionInfo>;

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

#[async_trait::async_trait]
impl ContenderRpcServer for ContenderServer {
    async fn status(&self) -> jsonrpsee::core::RpcResult<String> {
        let sessions = self.sessions.read().await;
        Ok(format!("{} session(s) active", sessions.num_sessions()))
    }

    async fn add_session(&self, name: String) -> jsonrpsee::core::RpcResult<ContenderSessionInfo> {
        let mut sessions = self.sessions.write().await;
        let id = sessions.add_session(name);
        Ok(id)
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
