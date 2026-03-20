use base64::DecodeError;
use jsonrpsee::types::{ErrorObject, ErrorObjectOwned};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ContenderRpcError {
    #[error("Failed to initialize contender session: {0}")]
    SessionInitializationFailed(contender_core::Error),

    #[error("Invalid test config: {0}")]
    InvalidTestConfig(#[from] contender_testfile::Error),

    #[error("Invalid arguments: {0}")]
    InvalidArguments(String),

    #[error("Invalid base64: {0}")]
    InvalidBase64(#[from] DecodeError),

    #[error("Invalid UTF-8 in decoded config: {0}")]
    InvalidUtf8(std::string::FromUtf8Error),
}

impl From<ContenderRpcError> for ErrorObjectOwned {
    fn from(err: ContenderRpcError) -> Self {
        match err {
            /* TODO
               standardize error codes and messages,
               and decide what info to include in the data field
               (e.g. stack traces for internal errors, but not for user errors)
            */
            ContenderRpcError::SessionInitializationFailed(e) => ErrorObject::owned(
                1,
                "Failed to initialize contender session".to_string(),
                Some(e.to_string()),
            ),

            ContenderRpcError::InvalidTestConfig(e) => {
                ErrorObject::owned(2, "Invalid test config".to_string(), Some(e.to_string()))
            }

            ContenderRpcError::InvalidBase64(e) => ErrorObject::owned(
                3,
                "Invalid base64 encoding".to_string(),
                Some(e.to_string()),
            ),

            ContenderRpcError::InvalidUtf8(e) => ErrorObject::owned(
                4,
                "Invalid UTF-8 in config".to_string(),
                Some(e.to_string()),
            ),

            ContenderRpcError::InvalidArguments(msg) => {
                ErrorObject::owned(400, "Invalid arguments".to_string(), Some(msg))
            }
        }
    }
}
