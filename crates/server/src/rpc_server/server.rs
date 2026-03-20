use contender_core::generator::RandSeed;
use jsonrpsee::{proc_macros::rpc, PendingSubscriptionSink, SubscriptionMessage};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, Instrument};

use crate::{
    error::ContenderRpcError,
    rpc_server::types::AddSessionParams,
    sessions::{ContenderSessionCache, ContenderSessionInfo, SessionStatus},
};

#[rpc(server)]
pub trait ContenderRpc {
    // ================ RPC Methods ================

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

    #[method(name = "spam")]
    async fn spam(&self, session_id: usize) -> jsonrpsee::core::RpcResult<String>;

    // ================ WS Methods ================

    #[subscription(name = "subscribe_logs" => "session_log", item = String)]
    async fn subscribe_logs(&self, session_id: usize) -> jsonrpsee::core::SubscriptionResult;
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

    async fn add_session(
        &self,
        params: AddSessionParams,
    ) -> jsonrpsee::core::RpcResult<ContenderSessionInfo> {
        let session_seed;
        let info;
        {
            let mut sessions = self.sessions.write().await;
            session_seed = RandSeed::seed_from_bytes(&sessions.num_sessions().to_be_bytes());
            let session = sessions.add_session(params.to_new_session_params(session_seed).await?);
            info = session.info.clone();
        }

        let session_id = info.id;
        let sessions = Arc::clone(&self.sessions);

        info!(
            "Spawning initialization for session {} with RPC URL {}",
            info.name, info.rpc_url
        );

        let span = tracing::info_span!("session_init", id = session_id);
        tokio::spawn(
            contender_core::CURRENT_SESSION_ID.scope(
                session_id,
                async move {
                    // Take the contender out so we can initialize without holding the lock.
                    let contender = {
                        let mut lock = sessions.write().await;
                        lock.take_contender(session_id)
                    };

                    let Some(mut contender) = contender else {
                        return;
                    };

                    let result = contender.initialize().await;

                    // Put the contender back and update status.
                    let mut lock = sessions.write().await;
                    lock.put_contender(session_id, contender);
                    if let Some(session) = lock.get_session_mut(session_id) {
                        match result {
                            Ok(()) => {
                                session.info.status = SessionStatus::Ready;
                                info!("Session {} initialized successfully", session_id);
                            }
                            Err(e) => {
                                let msg = e.to_string();
                                session.info.status = SessionStatus::Failed(msg.clone());
                                tracing::error!(
                                    "Session {} initialization failed: {}",
                                    session_id,
                                    msg
                                );
                            }
                        }
                    }
                }
                .instrument(span),
            ),
        );

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

    async fn subscribe_logs(
        &self,
        pending: PendingSubscriptionSink,
        session_id: usize,
    ) -> jsonrpsee::core::SubscriptionResult {
        let sessions = self.sessions.read().await; // TODO: replace self.sessions calls with wrappers to avoid accidental improper locking patterns
        let Some(session) = sessions.get_session(session_id) else {
            pending
                .reject(jsonrpsee::types::ErrorObject::owned(
                    5,
                    format!("Session {session_id} not found"),
                    None::<()>,
                ))
                .await;
            return Ok(());
        };
        let mut rx = session.log_channel.subscribe();
        drop(sessions);

        let sink = pending.accept().await?;

        tokio::spawn(async move {
            while let Ok(msg) = rx.recv().await {
                let sub_msg =
                    SubscriptionMessage::from_json(&msg).expect("failed to serialize log message");
                if sink.send(sub_msg).await.is_err() {
                    break;
                }
            }
        });

        Ok(())
    }

    async fn spam(&self, session_id: usize) -> jsonrpsee::core::RpcResult<String> {
        let sessions = self.sessions.read().await;
        let Some(session) = sessions.get_session(session_id) else {
            return Err(ContenderRpcError::SessionNotFound(session_id).into());
        };

        if session.info.status != SessionStatus::Ready {
            return Err(ContenderRpcError::SessionNotInitialized(session.info.clone()).into());
        }
        drop(sessions);

        let span = tracing::info_span!("session_spam", id = session_id);
        tokio::spawn(
            contender_core::CURRENT_SESSION_ID
                .scope(session_id, async move {
                    println!("spawned task for spamming session {session_id}");
                    // TODO: spam with contender here
                })
                .instrument(span),
        );

        Ok(format!("Spamming session {session_id}"))
    }
}
