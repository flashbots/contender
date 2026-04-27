use crate::server::{
    error::ContenderRpcError,
    log_layer::SessionLogSinks,
    rpc_server::{SessionOptions, SpamParams, SpammerType},
};
use contender_core::{
    agent_controller::AgentStore,
    alloy::{primitives::U256, signers::local::PrivateKeySigner},
    generator::{types::AnyProvider, RandSeed},
    test_scenario::Url,
    Contender, Initialized, Uninitialized,
};
use contender_sqlite::SqliteDb;
use contender_testfile::TestConfig;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

type SessionId = usize;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum SessionStatus {
    Initializing,
    Ready,
    Spamming(SpamParams),
    Failed(String),
}

impl std::fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionStatus::Initializing => write!(f, "Initializing"),
            SessionStatus::Ready => write!(f, "Ready"),
            SessionStatus::Spamming(params) => {
                let run_opts = params.as_run_opts();
                let spammer_type = params.spammer.clone().unwrap_or_default();
                let units = match spammer_type {
                    SpammerType::Timed => ("tps", "seconds"),
                    SpammerType::Blockwise => ("tpb", "blocks"),
                };
                write!(
                    f,
                    "Spamming ({} {} for {} {})",
                    run_opts.txs_per_period, units.0, run_opts.periods, units.1
                )
            }
            SessionStatus::Failed(err) => write!(f, "Failed: {err}"),
        }
    }
}

type InitContender =
    Contender<SqliteDb, RandSeed, TestConfig, Initialized<SqliteDb, RandSeed, TestConfig>>;

/// Wraps the two possible lifecycle states of a session's `Contender`.
pub enum SessionContender {
    Uninit(Contender<SqliteDb, RandSeed, TestConfig, Uninitialized>),
    Init(InitContender),
}

pub struct ContenderSession {
    /// Metadata about this session (id, name, rpc_url, status).
    pub info: ContenderSessionInfo,
    /// The contender instance for this session. `None` while it is taken out for
    /// initialization or spamming (to avoid holding the lock during long operations).
    pub contender: Option<SessionContender>,
    /// Broadcast channel for per-session log lines. The tracing layer sends formatted
    /// events here; WS and SSE subscribers receive from it.
    pub log_channel: broadcast::Sender<String>,
    /// Session-lifetime token. Cancelled when the session is removed, which terminates
    /// all WS/SSE log subscriber tasks. Once cancelled the session cannot be reused.
    pub cancel: CancellationToken,
    /// Per-spam-run token. Created fresh each time `spam` is called, cancelled by `stop`
    /// (or `remove`). After cancellation the session returns to `Ready` and can spam again.
    pub spam_cancel: Option<CancellationToken>,

    // --- Cached funding data (available even while contender is taken) ---
    /// The funder signer, populated after initialization.
    pub funder: Option<PrivateKeySigner>,
    /// The agent store, populated after initialization.
    pub agent_store: Option<AgentStore>,
    /// The RPC client, populated after initialization.
    pub rpc_client: Option<Arc<AnyProvider>>,
    /// The configured minimum balance for agent accounts.
    pub min_balance: Option<U256>,
}

/// Params for creating a new session ([ContenderSession::new]).
pub struct NewSessionParams {
    pub name: String,
    pub rpc_url: Url,
    pub test_config: TestConfig,
    pub options: SessionOptions,
}

impl ContenderSession {
    /// Should only be called by ContenderSessionCache when adding a new session,
    /// since the session ID is determined by the cache
    async fn new(id: SessionId, params: NewSessionParams) -> Result<Self, ContenderRpcError> {
        let info = ContenderSessionInfo {
            id,
            name: params.name,
            rpc_url: params.rpc_url,
            status: SessionStatus::Initializing,
        };

        let contender = info
            .create_contender(params.test_config, params.options)
            .await?;
        let (log_channel, _) = broadcast::channel(4096);
        let cancel = contender.cancel_token();
        Ok(Self {
            info,
            contender: Some(SessionContender::Uninit(contender)),
            log_channel,
            cancel,
            spam_cancel: None,
            funder: None,
            agent_store: None,
            rpc_client: None,
            min_balance: None,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ContenderSessionInfo {
    pub id: SessionId,
    pub name: String,
    pub rpc_url: Url,
    pub status: SessionStatus,
}

impl ContenderSessionInfo {
    pub async fn create_contender(
        &self,
        testconfig: TestConfig,
        options: SessionOptions,
    ) -> Result<Contender<SqliteDb, RandSeed, TestConfig, Uninitialized>, ContenderRpcError> {
        // using in-memory SQLite for now; will switch to file-based if we need persistence across server restarts
        let db = contender_sqlite::SqliteDb::new_memory();
        let seeder = contender_core::generator::RandSeed::seed_from_bytes(&self.id.to_be_bytes());

        // add env to TestConfig before building ContenderCtx
        let mut testconfig = testconfig;
        if let Some(env) = options.env {
            let og_env = testconfig.env.clone().unwrap_or_default();
            let full_env: HashMap<_, _> = og_env.into_iter().chain(env).collect();
            testconfig = testconfig.with_env(full_env);
        }

        // build contender context
        let mut ctx_builder =
            contender_core::ContenderCtx::builder(testconfig, db, seeder, self.rpc_url.clone())
                .scenario_label(format!("{}_{}", self.name, self.id));

        // apply options to contender context
        if let Some(auth) = options.auth {
            let auth_provider = auth.new_provider().await?;
            ctx_builder = ctx_builder.auth_provider(Arc::new(auth_provider));
        }
        if let Some(builder) = options.builder {
            ctx_builder = ctx_builder
                .builder_rpc_url(builder.rpc_url)
                .bundle_type(builder.bundle_type.into());
        }
        if let Some(min_balance) = options.min_balance {
            ctx_builder = ctx_builder.funding(min_balance);
        }
        if let Some(timeout) = options.pending_tx_timeout {
            ctx_builder = ctx_builder.pending_tx_timeout(timeout);
        }
        if let Some(tx_type) = options.tx_type {
            ctx_builder = ctx_builder.tx_type(tx_type.into());
        }
        if let Some(keys) = options.private_keys {
            let signers = {
                let mut signers = vec![];
                for key in keys {
                    let signer = PrivateKeySigner::from_bytes(&key).map_err(|e| {
                        ContenderRpcError::InvalidArguments(format!(
                            "invalid private key detected: {e}"
                        ))
                    })?;
                    signers.push(signer);
                }
                signers
            };
            ctx_builder = ctx_builder.user_signers(signers);
        }
        if let Some(agent_params) = options.agents {
            ctx_builder = ctx_builder.agent_spec(agent_params.into());
        }

        // build context and return contender instance
        Ok(ctx_builder.build().create_contender())
    }
}

pub struct ContenderSessionCache {
    sessions: Vec<ContenderSession>,
    log_sinks: SessionLogSinks,
}

impl ContenderSessionCache {
    pub fn new(log_sinks: SessionLogSinks) -> Self {
        Self {
            sessions: Vec::new(),
            log_sinks,
        }
    }

    /// Generate a random session ID that is not currently in use. This is simpler than tracking used IDs and reusing them, and the ID space is large enough that collisions should be extremely rare.
    /// In case of collision, we simply try again with a new random ID by recursing.
    fn gen_id(&self) -> SessionId {
        let id = rand::random::<u32>() as SessionId;
        if self.sessions.iter().all(|s| s.info.id != id) {
            id
        } else {
            self.gen_id()
        }
    }

    /// Add a new session to the cache. The ID is simply the index of the session in the vector.
    /// The session is not initialized yet, the caller is responsible for calling initialize on the session's contender before it's returned by the RPC provider.
    ///
    /// Returns a mutable reference to the newly added session,
    /// which can be used to call initialize on it before it's returned by the RPC provider.
    pub async fn add_session(
        &mut self,
        params: NewSessionParams,
    ) -> Result<&mut ContenderSession, ContenderRpcError> {
        let session = ContenderSession::new(self.gen_id(), params).await?;
        let info = session.info.clone();
        let log_channel = session.log_channel.clone();

        // Register the broadcast sender in the log sinks so the tracing layer can route to it.
        if let Ok(mut sinks) = self.log_sinks.try_write() {
            sinks.insert(info.id, log_channel);
        }

        self.sessions.push(session);
        Ok(self.sessions.last_mut().expect("just pushed, should exist"))
    }

    pub fn get_session(&self, id: SessionId) -> Option<&ContenderSession> {
        self.sessions.iter().find(|s| s.info.id == id)
    }

    pub fn get_session_mut(&mut self, id: SessionId) -> Option<&mut ContenderSession> {
        self.sessions.iter_mut().find(|s| s.info.id == id)
    }

    /// Take an uninitialized Contender out of a session so it can be initialized outside the lock.
    pub fn take_uninitialized(
        &mut self,
        id: SessionId,
    ) -> Option<Contender<SqliteDb, RandSeed, TestConfig, Uninitialized>> {
        let session = self.get_session_mut(id)?;
        match session.contender.take() {
            Some(SessionContender::Uninit(c)) => Some(c),
            other => {
                // put it back if it was the wrong variant
                session.contender = other;
                None
            }
        }
    }

    /// Take an initialized Contender out of a session so it can be used outside the lock.
    pub fn take_initialized(&mut self, id: SessionId) -> Option<InitContender> {
        let session = self.get_session_mut(id)?;
        match session.contender.take() {
            Some(SessionContender::Init(c)) => Some(c),
            other => {
                // put it back if it was the wrong variant
                session.contender = other;
                None
            }
        }
    }

    /// Put an initialized Contender back into a session.
    /// Also caches funding-related data so `fund_accounts` can work while the
    /// contender is taken for spamming.
    pub fn put_initialized(&mut self, id: SessionId, contender: InitContender) {
        if let Some(session) = self.get_session_mut(id) {
            let scenario = contender.scenario();
            session.funder = contender.funder().cloned();
            session.agent_store = Some(scenario.agent_store.clone());
            session.rpc_client = Some(scenario.rpc_client.clone());
            session.min_balance = Some(contender.min_balance());
            session.contender = Some(SessionContender::Init(contender));
        }
    }

    pub async fn remove_session(&mut self, id: SessionId) {
        // If the session exists and has an initialized Contender, shut it down first.
        if let Some(session) = self.get_session_mut(id) {
            // Stop any running spam before tearing down.
            if let Some(ref token) = session.spam_cancel {
                token.cancel();
            }

            // If the session has an initialized Contender, take it and shut it down.
            let maybe_contender = match session.contender.take() {
                Some(SessionContender::Init(c)) => Some(c),
                Some(SessionContender::Uninit(c)) => {
                    c.cancel();
                    None
                }
                other => {
                    session.contender = other;
                    None
                }
            };
            if let Some(mut contender) = maybe_contender {
                // Call shutdown on the scenario to stop all background actors.
                // This is async, so we must await it.
                let scenario = contender.scenario_mut();
                scenario.shutdown().await;
            }

            // Cancel subscriber streams before dropping the session.
            session.cancel.cancel();
        }
        // Deregister the log sink.
        if let Ok(mut sinks) = self.log_sinks.try_write() {
            sinks.remove(&id);
        }
        self.sessions.retain(|s| s.info.id != id);
    }

    pub fn all_sessions(&self) -> Vec<ContenderSessionInfo> {
        self.sessions.iter().map(|s| s.info.clone()).collect()
    }

    pub fn num_sessions(&self) -> usize {
        self.sessions.len()
    }
}
