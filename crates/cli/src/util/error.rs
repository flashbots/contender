use alloy::{
    primitives::{utils::format_ether, Address, U256},
    transports::{RpcError, TransportErrorKind},
};
use op_alloy_network::{Ethereum, TransactionBuilderError};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum UtilError {
    #[error("env error")]
    EnvVar(#[from] std::env::VarError),

    #[error("io error")]
    Io(#[from] std::io::Error),

    #[error("invalid scenario path: {0}")]
    InvalidScenarioPath(String),

    #[error("failed to parse duration")]
    ParseDuration(#[from] ParseDurationError),

    #[error("rpc error")]
    Rpc(#[from] RpcError<TransportErrorKind>),

    #[error("failed to build tx")]
    BuildTxFailed(#[from] TransactionBuilderError<Ethereum>),

    #[error("failed to make a backup of the DB: {0}")]
    DBBackupFailed(std::io::Error),

    #[error("failed to import DB from file: {0}")]
    DBImportFailed(std::io::Error),

    #[error("source database file does not exist")]
    DBDoesNotExist,

    #[error("failed to export database: {0}")]
    DBExportFailed(std::io::Error),

    #[error(
        "User account {sender} has insufficient balance to fund all spammer accounts. Have {}, needed {}. Chain ID: {chain_id}",
        format_ether(*have),
        format_ether(*need)
    )]
    InsufficientUserFunds {
        sender: Address,
        have: U256,
        need: U256,
        chain_id: u64,
    },
}

#[derive(Debug, Error)]
pub enum ParseDurationError {
    #[error("unexpected digit after unit in '{0}'")]
    UnexpectedDigit(String),

    #[error("invalid duration ('{0}'): floating point values are not supported")]
    NoFloats(String),

    #[error("invalid duration ('{0}'): could not parse number into u64")]
    InvalidNumber(String),

    #[error("invalid duration units: '{0}'.  Supported units: ms, msec, millisecond(s), s, sec(s), second(s), m, min(ute)(s), h, hr(s), hour(s), d, day(s).")]
    InvalidUnits(String),
}

impl UtilError {
    pub fn insufficient_user_funds(sender: Address, have: U256, need: U256, chain_id: u64) -> Self {
        Self::InsufficientUserFunds {
            sender,
            have,
            need,
            chain_id,
        }
    }
}
