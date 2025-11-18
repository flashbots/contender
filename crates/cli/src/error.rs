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
use miette::Diagnostic;
use thiserror::Error;
use tokio::task;

#[derive(Debug, Error, Diagnostic)]
pub enum ContenderError {
    #[error("invalid CLI params")]
    CliParamsInvalid(#[from] RuntimeParamErrorKind),

    #[error("auth provider error")]
    AuthProvider(#[from] AuthProviderError),

    #[error("invalid arg(s)")]
    Args(#[from] ArgsError),

    #[error("admin error")]
    Admin(#[from] AdminError),

    #[error("core error")]
    Core(#[from] contender_core::Error),

    #[error("custom contract args error")]
    CustomContractArgs(#[from] CustomContractArgsError),

    #[error("db error")]
    Db(#[from] contender_sqlite::Error),

    #[error("invalid DB version")]
    DbVersion,

    #[error("io error")]
    Io(#[from] std::io::Error),

    #[error("tokio task join error")]
    TaskJoin(#[from] task::JoinError),

    #[error("failed to parse hex value")]
    ParseHex(#[from] FromHexError),

    #[error("testfile error")]
    TestFile(#[from] contender_testfile::Error),

    #[error("report error")]
    Report(#[from] contender_report::Error),

    #[error("rpc error")]
    Rpc(#[from] RpcError<TransportErrorKind>),

    #[error("setup error")]
    Setup(#[from] SetupError),

    #[error("util error")]
    Util(#[from] UtilError),
}
