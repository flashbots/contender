use std::error::Error;

pub enum ContenderError {
    DbError(&'static str, Option<String>),
    SpamError(&'static str, Option<String>),
    SetupError(&'static str, Option<String>),
    GenericError(&'static str, String),
}

impl ContenderError {
    pub fn with_err(err: impl Error, msg: &'static str) -> Self {
        ContenderError::GenericError(msg, format!("{:?}", err))
    }
}

impl std::fmt::Display for ContenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ContenderError::SpamError(msg, _) => write!(f, "SpamError: {}", msg),
            ContenderError::DbError(msg, _) => write!(f, "DatabaseError: {}", msg),
            ContenderError::GenericError(msg, e) => {
                write!(f, "{} {}", msg, e.to_owned())
            }
            ContenderError::SetupError(msg, _) => write!(f, "SetupError: {}", msg),
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
            ContenderError::SetupError(msg, e) => {
                write!(f, "SetupError: {} {}", msg, err(e.to_owned()))
            }
            ContenderError::GenericError(msg, e) => {
                write!(f, "{} {}", msg, e.to_owned())
            }
        }
    }
}

impl Error for ContenderError {}
