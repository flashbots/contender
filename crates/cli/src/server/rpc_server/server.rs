use contender_core::generator::RandSeed;
use contender_core::spammer::{BlockwiseSpammer, LogCallback, NilCallback, TimedSpammer};
use futures::FutureExt;
use jsonrpsee::{proc_macros::rpc, PendingSubscriptionSink, SubscriptionMessage};
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn, Instrument};

use crate::server::{
    error::ContenderRpcError,
    rpc_server::{AddSessionParams, FundAccountsParams, ServerStatus, SpamParams, SpammerType},
    sessions::{ContenderSession, ContenderSessionCache, ContenderSessionInfo, SessionStatus},
};

#[rpc(server)]
pub trait ContenderRpc {
    // ================ RPC Methods ================

    #[method(name = "status")]
    async fn status(&self) -> jsonrpsee::core::RpcResult<ServerStatus>;

    #[method(name = "addSession")]
    async fn add_session(
        &self,
        name: AddSessionParams,
    ) -> jsonrpsee::core::RpcResult<ContenderSessionInfo>;

    #[method(name = "getSession")]
    async fn get_session(
        &self,
        id: usize,
    ) -> jsonrpsee::core::RpcResult<Option<ContenderSessionInfo>>;

    #[method(name = "getAllSessions")]
    async fn get_all_sessions(&self) -> jsonrpsee::core::RpcResult<Vec<ContenderSessionInfo>>;

    #[method(name = "removeSession")]
    async fn remove_session(&self, id: usize) -> jsonrpsee::core::RpcResult<()>;

    #[method(name = "spam")]
    async fn spam(&self, params: SpamParams) -> jsonrpsee::core::RpcResult<String>;

    #[method(name = "stop")]
    async fn stop(&self, session_id: usize) -> jsonrpsee::core::RpcResult<String>;

    #[method(name = "fundAccounts")]
    async fn fund_accounts(&self, params: FundAccountsParams)
        -> jsonrpsee::core::RpcResult<String>;

    // ================ WS Methods ================

    #[subscription(name = "subscribeLogs" => "session_log", item = String)]
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
    async fn status(&self) -> jsonrpsee::core::RpcResult<ServerStatus> {
        let sessions = self.sessions.read().await;
        Ok(ServerStatus {
            num_sessions: sessions.num_sessions(),
        })
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
            let session = sessions
                .add_session(params.to_new_session_params(session_seed).await?)
                .await?;
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
                    // Take contender instance so we can initialize without holding the lock.
                    let contender = {
                        let mut lock = sessions.write().await;
                        lock.take_contender(session_id)
                    };

                    let Some(mut contender) = contender else {
                        return;
                    };

                    let result = contender.initialize().await;

                    // Put contender back and update status.
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

    async fn get_all_sessions(&self) -> jsonrpsee::core::RpcResult<Vec<ContenderSessionInfo>> {
        let sessions = self.sessions.read().await;
        Ok(sessions.all_sessions())
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
        let cancel = session.cancel.clone();
        drop(sessions);

        let sink = pending.accept().await?;

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    result = rx.recv() => {
                        let Ok(msg) = result else { break };
                        let sub_msg =
                            SubscriptionMessage::from_json(&msg).expect("failed to serialize log message");
                        if sink.send(sub_msg).await.is_err() {
                            break;
                        }
                    }
                    _ = cancel.cancelled() => break,
                }
            }
        });

        Ok(())
    }

    async fn spam(&self, params: SpamParams) -> jsonrpsee::core::RpcResult<String> {
        let session_id = params.session_id;
        let sessions = self.sessions.read().await;
        let Some(session) = sessions.get_session(session_id) else {
            return Err(ContenderRpcError::SessionNotFound(session_id).into());
        };
        error_if_session_not_ready(&session)?;
        let save_receipts = params.save_receipts.unwrap_or(false);
        drop(sessions);

        // Take contender instance so we can spam without holding the `sessions` lock.
        let spam_cancel = CancellationToken::new();
        let contender = {
            let mut lock = self.sessions.write().await;
            if let Some(session) = lock.get_session_mut(session_id) {
                session.info.status = SessionStatus::Spamming(params.clone());
                session.spam_cancel = Some(spam_cancel.clone());
            }
            lock.take_contender(session_id)
        };

        let Some(contender) = contender else {
            return Err(ContenderRpcError::SessionNotFound(session_id).into());
        };

        let sessions = Arc::clone(&self.sessions);
        let sessions_panic = Arc::clone(&self.sessions);

        let opts = params.as_run_opts();
        let spammer_type = params.spammer.unwrap_or_default();
        let run_forever = params.run_forever.unwrap_or(false);

        // Set up background funding for run_forever mode.
        // The spam loop sends () on `fund_tx` after each batch; the funding
        // task receives it and tops up any spammer account whose balance has
        // dropped to within 10% of `min_balance`.
        let fund_tx = if run_forever {
            let (fund_tx, fund_rx) = tokio::sync::mpsc::channel::<()>(1);

            // Grab cached funding data under a brief read lock.
            let funding_data = {
                let sessions = self.sessions.read().await;
                sessions.get_session(session_id).and_then(|s| {
                    Some((
                        s.funder.clone()?,
                        s.agent_store.clone()?,
                        s.rpc_client.clone()?,
                        s.min_balance?,
                    ))
                })
            };

            if let Some((funder, agent_store, rpc_client, min_balance)) = funding_data {
                let cancel = spam_cancel.clone();
                tokio::spawn(
                    contender_core::CURRENT_SESSION_ID.scope(
                        session_id,
                        async move {
                            run_funding_loop(
                                fund_rx,
                                cancel,
                                funder,
                                agent_store,
                                rpc_client,
                                min_balance,
                            )
                            .await;
                        }
                        .instrument(tracing::info_span!("session_funding", id = session_id)),
                    ),
                );
            }

            Some(fund_tx)
        } else {
            None
        };

        // spam in background so we can update session status and log results without blocking the RPC response
        tokio::spawn(
            contender_core::CURRENT_SESSION_ID.scope(
                session_id,
                async move {
                    let inner = AssertUnwindSafe(async {
                        let mut contender = contender;
                        let fund_tx = fund_tx;

                        macro_rules! run_spam {
                            ($callback:expr) => {{
                                let callback = Arc::new($callback);

                                loop {
                                    let res = match spammer_type {
                                        SpammerType::Timed => {
                                            let spammer = TimedSpammer::new(Duration::from_secs(1));
                                            contender
                                                .spam(
                                                    spammer,
                                                    Arc::clone(&callback),
                                                    opts.clone(),
                                                    Some(spam_cancel.clone()),
                                                )
                                                .await
                                        }
                                        SpammerType::Blockwise => {
                                            let spammer = BlockwiseSpammer::new();
                                            contender
                                                .spam(
                                                    spammer,
                                                    Arc::clone(&callback),
                                                    opts.clone(),
                                                    Some(spam_cancel.clone()),
                                                )
                                                .await
                                        }
                                    };
                                    if !run_forever || res.is_err() || spam_cancel.is_cancelled() {
                                        break res;
                                    }
                                    info!("run_forever: restarting spam for session {session_id}");
                                    // Signal the funding task to check balances.
                                    if let Some(ref tx) = fund_tx {
                                        let _ = tx.try_send(());
                                    }
                                }
                            }};
                        }

                        let result = if save_receipts {
                            let provider = contender.provider();
                            run_spam!(LogCallback::new(Arc::new(provider)))
                        } else {
                            run_spam!(NilCallback)
                        };

                        // Clear pending tx cache and sync nonces before returning the contender.
                        if let Some(scenario) = contender.state.scenario_mut() {
                            for handle in scenario.msg_handles.values() {
                                if let Err(e) = handle.clear_cache().await {
                                    warn!("Failed to clear pending tx cache: {e}");
                                }
                            }
                            if let Err(e) = scenario.sync_nonces().await {
                                warn!("Failed to sync nonces after stop: {e}");
                            }
                        }

                        // Put contender back and log outcome.
                        let mut lock = sessions.write().await;
                        lock.put_contender(session_id, contender);
                        if let Some(session) = lock.get_session_mut(session_id) {
                            session.spam_cancel = None;
                        }
                        match result {
                            Ok(()) => {
                                if let Some(session) = lock.get_session_mut(session_id) {
                                    session.info.status = SessionStatus::Ready;
                                }
                                info!("Session {} spam completed successfully", session_id);
                            }
                            Err(e) => {
                                if let Some(session) = lock.get_session_mut(session_id) {
                                    session.info.status =
                                        SessionStatus::Failed(format!("spam failed: {e}"));
                                }
                                tracing::error!("Session {} spam failed: {}", session_id, e);
                            }
                        }
                    });

                    // Catch panics so the session always transitions out of Spamming.
                    if let Err(panic_info) = inner.catch_unwind().await {
                        let msg = match panic_info.downcast_ref::<&str>() {
                            Some(s) => s.to_string(),
                            None => match panic_info.downcast_ref::<String>() {
                                Some(s) => s.clone(),
                                None => "unknown panic".to_string(),
                            },
                        };
                        let mut lock = sessions_panic.write().await;
                        if let Some(session) = lock.get_session_mut(session_id) {
                            session.spam_cancel = None;
                            session.info.status =
                                SessionStatus::Failed(format!("spam panicked: {msg}"));
                        }
                        tracing::error!("Session {} spam panicked: {}", session_id, msg);
                    }
                }
                .instrument(tracing::info_span!("session_spam", id = session_id)),
            ),
        );

        Ok(format!("Spamming session {session_id}"))
    }

    async fn stop(&self, session_id: usize) -> jsonrpsee::core::RpcResult<String> {
        let span = tracing::info_span!("session_stop", id = session_id);
        let sessions = self.sessions.read().await;
        let Some(session) = sessions.get_session(session_id) else {
            return Err(ContenderRpcError::SessionNotFound(session_id).into());
        };
        let Some(ref token) = session.spam_cancel else {
            return Err(ContenderRpcError::SessionNotBusy(session_id).into());
        };
        token.cancel();
        drop(sessions);
        {
            let _enter = span.enter();
            info!("Sent stop signal to session {session_id}");
        }
        Ok(format!("Stopping session {session_id}"))
    }

    async fn fund_accounts(
        &self,
        params: FundAccountsParams,
    ) -> jsonrpsee::core::RpcResult<String> {
        let session_id = params.session_id;

        // Grab cached funding data under a brief read lock — available even
        // while the contender is taken out for spamming.
        let (funder, agent, rpc_client) =
            {
                let sessions = self.sessions.read().await;
                let Some(session) = sessions.get_session(session_id) else {
                    return Err(ContenderRpcError::SessionNotFound(session_id).into());
                };
                // Allow funding in any initialized state (Ready or Spamming).
                match &session.info.status {
                    SessionStatus::Failed(msg) => {
                        return Err(ContenderRpcError::SessionFailed {
                            info: session.info.clone(),
                            error: msg.to_owned(),
                        }
                        .into());
                    }
                    SessionStatus::Ready | SessionStatus::Spamming(_) => {}
                    _ => {
                        return Err(
                            ContenderRpcError::SessionNotInitialized(session.info.clone()).into(),
                        );
                    }
                }

                let funder = session.funder.clone().ok_or_else(|| {
                    ContenderRpcError::SessionNotInitialized(session.info.clone())
                })?;
                let rpc_client = session.rpc_client.clone().ok_or_else(|| {
                    ContenderRpcError::SessionNotInitialized(session.info.clone())
                })?;
                let agent_store = session.agent_store.as_ref().ok_or_else(|| {
                    ContenderRpcError::SessionNotInitialized(session.info.clone())
                })?;

                let agent_class = params.agent_class.unwrap_or_default();
                let agent = agent_store.get_class(&agent_class).cloned();
                (funder, agent, rpc_client)
            };

        let span = tracing::info_span!("session_fund_accounts", id = session_id);
        let sessions = Arc::clone(&self.sessions);
        tokio::spawn(
            contender_core::CURRENT_SESSION_ID.scope(
                session_id,
                async move {
                    let result = if let Some(agent) = agent {
                        agent
                            .fund_signers(&funder, params.amount, rpc_client.as_ref().clone())
                            .await
                    } else {
                        tracing::warn!("No agents found for requested class, skipping funding");
                        Ok(())
                    };

                    match result {
                        Ok(()) => {
                            info!("Session {} accounts funded successfully", session_id);
                        }
                        Err(e) => {
                            let mut lock = sessions.write().await;
                            if let Some(session) = lock.get_session_mut(session_id) {
                                session.info.status =
                                    SessionStatus::Failed(format!("funding accounts failed: {e}"));
                            }
                            tracing::error!(
                                "Session {} funding accounts failed: {}",
                                session_id,
                                e
                            );
                        }
                    }
                }
                .instrument(span),
            ),
        );

        Ok(format!("Funding accounts for session {session_id}"))
    }
}

/// Helper function to check if a session is ready to spam,
/// returning an appropriate RPC error if not.
fn error_if_session_not_ready(session: &ContenderSession) -> jsonrpsee::core::RpcResult<()> {
    Ok(match &session.info.status {
        SessionStatus::Failed(msg) => {
            return Err(ContenderRpcError::SessionFailed {
                info: session.info.clone(),
                error: msg.to_owned(),
            }
            .into())
        }
        SessionStatus::Spamming(_) => {
            return Err(ContenderRpcError::SessionBusy(session.info.clone()).into())
        }
        SessionStatus::Ready => (),
        _ => return Err(ContenderRpcError::SessionNotInitialized(session.info.clone()).into()),
    })
}

/// Background task that listens for "batch done" signals and funds spammer
/// accounts whose balance has dropped to within 10% of `min_balance`.
async fn run_funding_loop(
    mut fund_rx: tokio::sync::mpsc::Receiver<()>,
    cancel: CancellationToken,
    funder: contender_core::alloy::signers::local::PrivateKeySigner,
    agent_store: contender_core::agent_controller::AgentStore,
    rpc_client: Arc<contender_core::generator::types::AnyProvider>,
    min_balance: contender_core::alloy::primitives::U256,
) {
    use contender_core::agent_controller::AgentClass;
    use contender_core::alloy::providers::Provider;

    // Threshold: fund when balance < (min_balance + 25%)
    let threshold = min_balance + (min_balance / contender_core::alloy::primitives::U256::from(4));

    loop {
        tokio::select! {
            msg = fund_rx.recv() => {
                if msg.is_none() {
                    // Channel closed — spam loop exited.
                    break;
                }
            }
            _ = cancel.cancelled() => break,
        }

        let Some(spammers) = agent_store.get_class(&AgentClass::Spammer) else {
            debug!("no spammer agents found, skipping balance check");
            continue;
        };

        let addresses = spammers.all_addresses();
        let mut needs_funding = false;

        for addr in &addresses {
            match rpc_client.get_balance(*addr).await {
                Ok(balance) if balance < threshold => {
                    info!(
                        "spammer {} balance ({}) below threshold ({}), will fund",
                        addr, balance, threshold
                    );
                    needs_funding = true;
                    break;
                }
                Ok(_) => {}
                Err(e) => {
                    warn!("failed to check balance for {}: {}", addr, e);
                }
            }
        }

        if needs_funding {
            info!("funding spammer accounts (min_balance={})", min_balance);
            if let Err(e) = spammers
                .fund_signers(&funder, min_balance, rpc_client.as_ref().clone())
                .await
            {
                warn!("background funding failed: {}", e);
            } else {
                info!("background funding completed successfully");
            }
        }
    }
    debug!("funding loop exited");
}
