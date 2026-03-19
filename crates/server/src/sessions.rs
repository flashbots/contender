use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub struct ContenderSession {
    pub info: ContenderSessionInfo,
    // TODO: add contender stuff here (ContenderCtx?)
}

impl ContenderSession {
    fn new(name: String, sessions: &[ContenderSession]) -> Self {
        Self {
            info: ContenderSessionInfo {
                id: sessions.len(),
                name,
            },
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ContenderSessionInfo {
    pub id: usize,
    pub name: String,
}

#[derive(Debug)]
pub struct ContenderSessionCache {
    sessions: Vec<ContenderSession>,
}

impl ContenderSessionCache {
    pub fn new() -> Self {
        Self {
            sessions: Vec::new(),
        }
    }

    /// Add a new session to the cache and return its ID. The ID is simply the index of the session in the vector.
    pub fn add_session(
        &mut self,
        name: String, /* TODO: here we add TestScenario-related params */
    ) -> ContenderSessionInfo {
        let session = ContenderSession::new(name, &self.sessions);
        let info = session.info.clone();
        self.sessions.push(session);
        info
    }

    pub fn get_session(&self, id: usize) -> Option<&ContenderSession> {
        self.sessions.iter().find(|s| s.info.id == id)
    }

    pub fn remove_session(&mut self, id: usize) {
        self.sessions.retain(|s| s.info.id != id);
    }

    pub fn num_sessions(&self) -> usize {
        self.sessions.len()
    }
}
