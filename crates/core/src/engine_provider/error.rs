use std::fmt::Display;

use alloy::transports::TransportErrorKind;
use alloy_json_rpc::RpcError;

#[derive(Debug)]
pub enum AuthProviderError {
    InternalError(Option<&'static str>, Box<dyn std::error::Error>),
    ConnectionFailed(Box<dyn std::error::Error>),
    ExtraDataTooShort,
    GasLimitRequired,
}

impl std::error::Error for AuthProviderError {}

impl Display for AuthProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthProviderError::InternalError(m, e) => {
                if let Some(m) = m {
                    write!(f, "internal error ({m}): {e}")
                } else {
                    write!(f, "internal error: {e}")
                }
            }
            AuthProviderError::ConnectionFailed(e) => {
                write!(f, "failed to connect to auth provider: {e}")
            }
            AuthProviderError::ExtraDataTooShort => {
                write!(f, "extra_data of genesis block is too short")
            }
            AuthProviderError::GasLimitRequired => write!(f, "gasLimit parameter is required"),
        }
    }
}

fn parse_err_str(error: &str) -> AuthProviderError {
    if error.contains(&AuthProviderError::GasLimitRequired.to_string()) {
        return AuthProviderError::GasLimitRequired;
    }
    if error.contains(&AuthProviderError::ExtraDataTooShort.to_string()) {
        return AuthProviderError::ExtraDataTooShort;
    }
    // If the error is not one of the above, we assume it's an internal error
    AuthProviderError::InternalError(None, error.into())
}

fn parse_err(err: Box<dyn std::error::Error>) -> AuthProviderError {
    let error = err.to_string();
    parse_err_str(&error)
}

impl From<String> for AuthProviderError {
    fn from(err: String) -> Self {
        parse_err_str(&err)
    }
}

impl From<Box<dyn std::error::Error>> for AuthProviderError {
    fn from(err: Box<dyn std::error::Error>) -> Self {
        parse_err(err)
    }
}

impl From<RpcError<TransportErrorKind>> for AuthProviderError {
    fn from(err: RpcError<TransportErrorKind>) -> Self {
        parse_err(Box::new(err))
    }
}
