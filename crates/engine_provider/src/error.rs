use reth_node_api::EngineApiMessageVersion;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuthProviderError {
    #[error("auth provider failed to connect: {0}")]
    ConnectionFailed(#[from] alloy::transports::TransportError),

    #[error("failed to retrieve block {0} from RPC provider")]
    MissingBlock(u64),

    #[error("invalid txs; block must include full txs")]
    InvalidTxs,

    #[error("unknown error (internal): {0}")]
    Internal(String),

    #[error("invalid block range: {0}-{1}")]
    InvalidBlockRange(u64, u64),

    #[error("invalid start block (must be >1): {0}")]
    InvalidBlockStart(u64),

    #[error("invalid payload, tried sending message version {0:?}. {1:?}")]
    InvalidPayload(EngineApiMessageVersion, Option<&'static str>),

    #[error("extra_data of genesis block is too short")]
    ExtraDataTooShort,

    #[error("gasLimit parameter required")]
    GasLimitRequired,
}

fn parse_err_str(error: &str) -> AuthProviderError {
    if error.contains(&AuthProviderError::GasLimitRequired.to_string()) {
        return AuthProviderError::GasLimitRequired;
    }
    if error.contains(&AuthProviderError::ExtraDataTooShort.to_string()) {
        return AuthProviderError::ExtraDataTooShort;
    }
    // If the error is not one of the above, we assume it's an internal error
    AuthProviderError::Internal(error.to_string())
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
