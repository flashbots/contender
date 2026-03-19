#[derive(Debug)]
pub struct ContenderSession {
    pub id: usize,
    pub name: String,
    // TODO: add contender stuff here (ContenderCtx?)
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
    ) -> usize {
        let session = ContenderSession {
            id: self.sessions.len(),
            name,
        };
        let id = session.id;
        self.sessions.push(session);
        id
    }

    pub fn get_session(&self, id: usize) -> Option<&ContenderSession> {
        self.sessions.iter().find(|s| s.id == id)
    }

    pub fn remove_session(&mut self, id: usize) {
        self.sessions.retain(|s| s.id != id);
    }
}
