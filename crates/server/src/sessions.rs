use contender_core::{generator::RandSeed, test_scenario::Url, Contender};
use contender_sqlite::SqliteDb;
use contender_testfile::TestConfig;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use crate::{
    log_layer::SessionLogSinks,
    rpc_server::{SpamParams, SpammerType},
};

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
                let res = params.to_run_opts();
                let spammer_type = params.spammer.clone().unwrap_or_default();
                let units = match spammer_type {
                    SpammerType::Timed => ("tps", "seconds"),
                    SpammerType::Blockwise => ("tpb", "blocks"),
                };
                write!(
                    f,
                    "Spamming ({} {} for {} {})",
                    res.txs_per_period, units.0, res.periods, units.1
                )
            }
            SessionStatus::Failed(err) => write!(f, "Failed: {err}"),
        }
    }
}

pub struct ContenderSession {
    pub info: ContenderSessionInfo,
    pub contender: Option<Contender<SqliteDb, RandSeed, TestConfig>>,
    pub log_channel: broadcast::Sender<String>,
    /// Cancelled when the session is removed; subscriber tasks should select on this.
    pub cancel: CancellationToken,
    /// Cancelled to stop a running spam. Reset each time spam is started.
    pub spam_cancel: Option<CancellationToken>,
}

pub struct NewSessionParams {
    pub name: String,
    pub rpc_url: Url,
    pub test_config: TestConfig,
}

impl ContenderSession {
    /// Should only be called by ContenderSessionCache when adding a new session,
    /// since the session ID is determined by the cache
    fn new(id: usize, params: NewSessionParams) -> Self {
        let info = ContenderSessionInfo {
            id,
            name: params.name,
            rpc_url: params.rpc_url,
            status: SessionStatus::Initializing,
        };

        let contender = info.create_contender(params.test_config);
        let (log_channel, _) = broadcast::channel(4096);
        Self {
            info,
            contender: Some(contender),
            log_channel,
            cancel: CancellationToken::new(),
            spam_cancel: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ContenderSessionInfo {
    pub id: usize,
    pub name: String,
    pub rpc_url: Url,
    pub status: SessionStatus,
}

impl ContenderSessionInfo {
    pub fn create_contender(
        &self,
        testconfig: TestConfig,
    ) -> Contender<SqliteDb, RandSeed, TestConfig> {
        // using in-memory SQLite for now; will switch to file-based if we need persistence across server restarts
        let db = contender_sqlite::SqliteDb::new_memory();
        let seeder = contender_core::generator::RandSeed::seed_from_bytes(&self.id.to_be_bytes());
        let contender_ctx =
            contender_core::ContenderCtx::builder(testconfig, db, seeder, self.rpc_url.clone())
                .build();

        Contender::new(contender_ctx)
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

    pub fn next_session_id(&self) -> usize {
        self.sessions.len()
    }

    /// Add a new session to the cache. The ID is simply the index of the session in the vector.
    /// The session is not initialized yet, the caller is responsible for calling initialize on the session's contender before it's returned by the RPC provider.
    ///
    /// Returns a mutable reference to the newly added session,
    /// which can be used to call initialize on it before it's returned by the RPC provider.
    pub fn add_session(&mut self, params: NewSessionParams) -> &mut ContenderSession {
        let session = ContenderSession::new(self.next_session_id(), params);
        let info = session.info.clone();
        let log_channel = session.log_channel.clone();

        // Register the broadcast sender in the log sinks so the tracing layer can route to it.
        if let Ok(mut sinks) = self.log_sinks.try_write() {
            sinks.insert(info.id, log_channel);
        }

        self.sessions.push(session);
        &mut self.sessions[info.id]
    }

    pub fn get_session(&self, id: usize) -> Option<&ContenderSession> {
        self.sessions.iter().find(|s| s.info.id == id)
    }

    pub fn get_session_mut(&mut self, id: usize) -> Option<&mut ContenderSession> {
        self.sessions.iter_mut().find(|s| s.info.id == id)
    }

    /// Take the Contender out of a session so it can be used outside the lock.
    pub fn take_contender(
        &mut self,
        id: usize,
    ) -> Option<Contender<SqliteDb, RandSeed, TestConfig>> {
        self.get_session_mut(id).and_then(|s| s.contender.take())
    }

    /// Put the Contender back into a session after initialization.
    pub fn put_contender(
        &mut self,
        id: usize,
        contender: Contender<SqliteDb, RandSeed, TestConfig>,
    ) {
        if let Some(session) = self.get_session_mut(id) {
            session.contender = Some(contender);
        }
    }

    pub fn remove_session(&mut self, id: usize) {
        // Cancel subscriber streams before dropping the session.
        if let Some(session) = self.get_session(id) {
            session.cancel.cancel();
        }
        // Deregister the log sink.
        if let Ok(mut sinks) = self.log_sinks.try_write() {
            sinks.remove(&id);
        }
        self.sessions.retain(|s| s.info.id != id);
    }

    pub fn num_sessions(&self) -> usize {
        self.sessions.len()
    }
}
