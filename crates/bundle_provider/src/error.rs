use alloy::transports::{RpcError, TransportErrorKind};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BundleProviderError {
    #[error("invalid builder URL")]
    InvalidUrl,

    #[error("failed to send bundle")]
    SendBundleError(#[from] RpcError<TransportErrorKind>),
}
