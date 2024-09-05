use std::error::Error;

pub enum ContenderError {
    DbError(&'static str, Option<String>),
    SpamError(&'static str, Option<String>),
}

impl std::fmt::Display for ContenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ContenderError::SpamError(msg, _) => write!(f, "SpamError: {}", msg),
            ContenderError::DbError(msg, _) => write!(f, "DatabaseError: {}", msg),
        }
    }
}

impl std::fmt::Debug for ContenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let err = |e: Option<String>| e.unwrap_or_default();
        match self {
            ContenderError::SpamError(msg, e) => {
                write!(f, "SpamError: {} {}", msg, err(e.to_owned()))
            }
            ContenderError::DbError(msg, e) => {
                write!(f, "DatabaseError: {} {}", msg, err(e.to_owned()))
            }
        }
    }
}

impl Error for ContenderError {}
