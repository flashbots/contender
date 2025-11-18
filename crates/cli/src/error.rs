use crate::{
    commands::{
        admin::AdminError,
        error::{ArgsError, SetupError},
    },
    default_scenarios::custom_contract::CustomContractArgsError,
    util::error::UtilError,
};
use alloy::{
    hex::FromHexError,
    transports::{RpcError, TransportErrorKind},
};
use contender_core::error::RuntimeParamErrorKind;
use contender_engine_provider::AuthProviderError;
use thiserror::Error;
use tokio::task;

#[derive(Debug, Error)]
pub enum ContenderError {
    #[error("invalid CLI params: {0}")]
    CliParamsInvalid(#[from] RuntimeParamErrorKind),

    #[error("auth provider error: {0}")]
    AuthProvider(#[from] AuthProviderError),

    #[error("invalid arg(s): {0}")]
    Args(#[from] ArgsError),

    #[error("{0}")]
    Admin(#[from] AdminError),

    #[error("{0}")]
    Core(#[from] contender_core::Error),

    #[error("{0}")]
    CustomContractArgs(#[from] CustomContractArgsError),

    #[error("db error: {0}")]
    Db(#[from] contender_sqlite::Error),

    #[error("invalid DB version")]
    DbVersion,

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("tokio task join error: {0}")]
    TaskJoin(#[from] task::JoinError),

    #[error("failed to parse hex value: {0}")]
    ParseHex(#[from] FromHexError),

    #[error("{0}")]
    TestFile(#[from] contender_testfile::Error),

    #[error("{0}")]
    Report(#[from] contender_report::Error),

    #[error("rpc error: {0}")]
    Rpc(#[from] RpcError<TransportErrorKind>),

    #[error("setup error: {0}")]
    Setup(#[from] SetupError),

    #[error("{0}")]
    Util(#[from] UtilError),
}

// pub type Result<T> = std::result::Result<T, ContenderError>;

// impl From<ContenderError> for &str {
//     fn from(value: ContenderError) -> Self {
//         todo!()
//     }
// }

// impl ContenderError {
//     pub fn with_err(err: impl Error, msg: &'static str) -> Self {
//         ContenderError::GenericError(msg, format!("{err:?}"))
//     }
// }

// impl std::fmt::Display for ContenderError {
//     fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
//         use ContenderError::*;

//         match self {
//             Admin(msg) => write!(f, "AdminError: {msg}"),
//             Db(msg) => write!(f, "DatabaseError: {msg}"),
//             Generic(msg) => {
//                 write!(f, "{}", msg, e.to_owned())
//             }
//             InvalidRuntimeParams(kind) => {
//                 write!(f, "InvalidRuntimeParams: {kind}")
//             }
//             Rpc(e) => {
//                 // let e = err.to_string().to_lowercase();
//                 // if e.contains("already known") {
//                 //     Error::RpcInternal(RpcErrorKind::TxAlreadyKnown)
//                 // } else if e.contains("insufficient funds") {
//                 //     ContenderError::RpcError(RpcErrorKind::InsufficientFunds(from), err)
//                 // } else if e.contains("replacement transaction underpriced") {
//                 //     ContenderError::RpcError(RpcErrorKind::ReplacementTransactionUnderpriced, err)
//                 // } else {
//                 //     RpcErrorKind::GenericSendTxError.to_error(err)
//                 // }
//                 write!(f, "RpcError: {e}")
//             }
//             Setup(msg, _) => write!(f, "SetupError: {msg}"),
//             Spam(msg, _) => write!(f, "SpamError: {msg}"),
//         }
//     }
// }

// impl std::fmt::Debug for ContenderError {
//     fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
//         let err = |e: Option<String>| e.unwrap_or_default();
//         match self {
//             ContenderError::AdminError(msg, e) => write!(f, "AdminError: {msg} - {e}"),
//             ContenderError::DbError(msg, e) => {
//                 write!(f, "DatabaseError: {} {}", msg, err(e.to_owned()))
//             }
//             ContenderError::GenericError(msg, e) => write!(f, "{msg} {e}"),
//             ContenderError::InvalidRuntimeParams(kind) => {
//                 write!(f, "InvalidRuntimeParams: {kind}")
//             }
//             ContenderError::RpcError(e) => {
//                 write!(f, "RpcError: {kind:?}: {e:?}")
//             }
//             ContenderError::SetupError(msg, e) => {
//                 write!(f, "SetupError: {} {}", msg, err(e.to_owned()))
//             }
//             ContenderError::SpamError(msg, e) => {
//                 write!(f, "SpamError: {} {}", msg, err(e.to_owned()))
//             }
//         }
//     }
// }

// impl Error for ContenderError {}

// impl From<DbError> for ContenderError {
//     fn from(value: DbError) -> Self {
//         Self::Db(())
//     }
// }
