use std::error::Error;

pub enum ContenderError {
    SpamError(String),
}

impl std::fmt::Display for ContenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ContenderError::SpamError(msg) => write!(f, "Spam error: {}", msg),
        }
    }
}

impl std::fmt::Debug for ContenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ContenderError::SpamError(msg) => write!(f, "Spam error: {}", msg),
        }
    }
}

impl Error for ContenderError {}

impl From<String> for ContenderError {
    fn from(msg: String) -> Self {
        ContenderError::SpamError(msg)
    }
}
