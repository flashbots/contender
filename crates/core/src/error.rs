use std::ops::Deref;
use std::{error::Error, fmt::Display};

use alloy::primitives::Address;
use alloy::transports::{RpcError, TransportErrorKind};
use contender_bundle_provider::error::BundleProviderError;

pub enum ContenderError {
    DbError(&'static str, Option<String>),
    SpamError(&'static str, Option<String>),
    SetupError(&'static str, Option<String>),
    GenericError(&'static str, String),
    AdminError(&'static str, String),
    InvalidRuntimeParams(RuntimeParamErrorKind),
    RpcError(RpcErrorKind, RpcError<TransportErrorKind>),
}

// #[derive(Debug)]
pub enum RpcErrorKind {
    TxAlreadyKnown,
    InsufficientFunds(Address),
    ReplacementTransactionUnderpriced,
    GenericSendTxError,
}

impl RpcErrorKind {
    pub fn to_error(self, e: RpcError<TransportErrorKind>) -> ContenderError {
        ContenderError::RpcError(self, e)
    }
}

#[derive(Debug)]
pub enum RuntimeParamErrorKind {
    BuilderUrlRequired,
    BuilderUrlInvalid,
    BundleTypeInvalid,
    MissingArgs(String),
}

impl std::fmt::Debug for RpcErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            RpcErrorKind::TxAlreadyKnown => write!(f, "Transaction already known. You may be using the same seed (or private key) as another spammer."),
            RpcErrorKind::InsufficientFunds(address) => write!(f, "Insufficient funds for transaction (from {address})."),
            RpcErrorKind::ReplacementTransactionUnderpriced => {
                write!(f, "Replacement transaction underpriced. You may have to wait, or replace the currently-pending transactions manually.")
            }
            RpcErrorKind::GenericSendTxError => write!(f, "Failed to send transaction. This may be due to a network issue or the transaction being invalid."),
        }
    }
}

impl Display for RuntimeParamErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            RuntimeParamErrorKind::BuilderUrlRequired => {
                write!(f, "builder URL is required")
            }
            RuntimeParamErrorKind::BuilderUrlInvalid => {
                write!(f, "invalid builder URL")
            }
            RuntimeParamErrorKind::BundleTypeInvalid => {
                write!(f, "invalid bundle type")
            }
            RuntimeParamErrorKind::MissingArgs(arg) => {
                write!(f, "missing required argument: {arg}")
            }
        }
    }
}

impl From<RuntimeParamErrorKind> for ContenderError {
    fn from(err: RuntimeParamErrorKind) -> ContenderError {
        ContenderError::InvalidRuntimeParams(err)
    }
}

impl From<BundleProviderError> for ContenderError {
    fn from(err: BundleProviderError) -> ContenderError {
        match err {
            BundleProviderError::InvalidUrl => {
                ContenderError::InvalidRuntimeParams(RuntimeParamErrorKind::BuilderUrlInvalid)
            }
            BundleProviderError::SendBundleError(e) => {
                if e.to_string()
                    .contains("bundle must contain exactly one transaction")
                {
                    return RuntimeParamErrorKind::BundleTypeInvalid.into();
                }
                ContenderError::with_err(e.deref(), "failed to send bundle")
            }
        }
    }
}

impl ContenderError {
    pub fn with_err(err: impl Error, msg: &'static str) -> Self {
        ContenderError::GenericError(msg, format!("{err:?}"))
    }
}

impl std::fmt::Display for ContenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ContenderError::AdminError(msg, e) => write!(f, "AdminError: {msg} - {e}"),
            ContenderError::DbError(msg, _) => write!(f, "DatabaseError: {msg}"),
            ContenderError::GenericError(msg, e) => {
                write!(f, "{} {}", msg, e.to_owned())
            }
            ContenderError::InvalidRuntimeParams(kind) => {
                write!(f, "InvalidRuntimeParams: {kind}")
            }
            ContenderError::RpcError(kind, e) => {
                write!(f, "RpcError: {kind:?}: {e}")
            }
            ContenderError::SetupError(msg, _) => write!(f, "SetupError: {msg}"),
            ContenderError::SpamError(msg, _) => write!(f, "SpamError: {msg}"),
        }
    }
}

impl std::fmt::Debug for ContenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let err = |e: Option<String>| e.unwrap_or_default();
        match self {
            ContenderError::AdminError(msg, e) => write!(f, "AdminError: {msg} - {e}"),
            ContenderError::DbError(msg, e) => {
                write!(f, "DatabaseError: {} {}", msg, err(e.to_owned()))
            }
            ContenderError::GenericError(msg, e) => write!(f, "{msg} {e}"),
            ContenderError::InvalidRuntimeParams(kind) => {
                write!(f, "InvalidRuntimeParams: {kind}")
            }
            ContenderError::RpcError(kind, e) => {
                write!(f, "RpcError: {kind:?}: {e:?}")
            }
            ContenderError::SetupError(msg, e) => {
                write!(f, "SetupError: {} {}", msg, err(e.to_owned()))
            }
            ContenderError::SpamError(msg, e) => {
                write!(f, "SpamError: {} {}", msg, err(e.to_owned()))
            }
        }
    }
}

impl Error for ContenderError {}
