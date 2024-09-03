use std::error::Error;

pub enum ContenderError {
    DbError(&'static str),
    SpamError(&'static str),
}

impl std::fmt::Display for ContenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ContenderError::SpamError(msg) => write!(f, "Spam error: {}", msg),
            ContenderError::DbError(msg) => write!(f, "Database error: {}", msg),
        }
    }
}

impl std::fmt::Debug for ContenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ContenderError::SpamError(msg) => write!(f, "Spam error: {}", msg),
            ContenderError::DbError(msg) => write!(f, "Database error: {}", msg),
        }
    }
}

impl Error for ContenderError {}
