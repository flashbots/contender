use contender_core::{generator::RandSeed, test_scenario::Url, Contender};
use contender_sqlite::SqliteDb;
use contender_testfile::TestConfig;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum SessionStatus {
    Initializing,
    Ready,
    Failed(String),
}

pub struct ContenderSession {
    pub info: ContenderSessionInfo,
    pub contender: Option<Contender<SqliteDb, RandSeed, TestConfig>>,
}

pub struct NewSessionParams {
    pub name: String,
    pub rpc_url: Url,
    pub test_config: TestConfig,
}

impl ContenderSession {
    /// Should only be called by ContenderSessionCache when adding a new session, since the session ID is determined by the cache
    fn new(sessions: &[ContenderSession], params: NewSessionParams) -> Self {
        let info = ContenderSessionInfo {
            id: sessions.len(),
            name: params.name,
            rpc_url: params.rpc_url,
            status: SessionStatus::Initializing,
        };

        let contender = info.create_contender(params.test_config);
        Self {
            info,
            contender: Some(contender),
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
}

impl ContenderSessionCache {
    pub fn new() -> Self {
        Self {
            sessions: Vec::new(),
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
        let session = ContenderSession::new(&self.sessions, params);
        let info = session.info.clone();

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
        self.sessions.retain(|s| s.info.id != id);
    }

    pub fn num_sessions(&self) -> usize {
        self.sessions.len()
    }
}
