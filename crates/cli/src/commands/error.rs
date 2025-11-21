use alloy::{
    primitives::{utils::format_ether, Address, TxHash, U256},
    providers::PendingTransactionError,
};
use std::path::PathBuf;
use thiserror::Error;

use crate::{
    commands::common::TxTypeCli,
    util::{bold, error::UtilError},
};

#[derive(Debug, Error)]
pub enum ArgsError {
    #[error("{} is required to send bundles", bold("--builder-url"))]
    BuilderUrlRequiredForBundles,

    #[error(
        "Invalid bundle type for this RPC. Set a different bundle type with {}",
        bold("--bundle-type")
    )]
    BundleTypeInvalid,

    #[error("engine args required for forkchoice {}",
        [(auth_rpc_url.is_none(), "--auth-rpc-url"), (jwt_secret.is_none(), "--jwt-secret")]
        .into_iter().map(|(missing, name)| {
            if missing { name.to_owned() } else { "".to_owned() }
        }).reduce(|a, e| format!("{a}, {e}")).unwrap_or_default()
    )]
    EngineArgsRequired {
        auth_rpc_url: Option<String>,
        jwt_secret: Option<PathBuf>,
    },

    #[error("engine provider must be initialized: {0}")]
    EngineProviderUninitialized(String),

    #[error(
        "Insufficient minimum balance: {} ETH. Set --min-balance to {} or higher.",
        format_ether(*min_balance),
        format_ether(*required_balance)
    )]
    MinBalanceInsufficient {
        min_balance: U256,
        required_balance: U256,
    },

    #[error("cannot use both scenario file and builtin scenario")]
    ScenarioFileBuiltinConflict,

    #[error("no spam calls were found in the scenario config, nothing to do")]
    SpamNotFound,

    #[error(
        "Either {} or {} must be set.",
        bold("--txs-per-block"),
        bold("--txs-per-second")
    )]
    SpamRateNotFound,

    #[error(
        "Not enough transactions per duration to cover all spam transactions.\nSet {} or {} to at least {min_tpd}",
        bold("--txs-per-block (--tpb)"),
        bold("--txs-per-second (--tps)")
    )]
    TransactionsPerDurationInsufficient { min_tpd: u64 },

    #[error(
        "invalid tx type for blob transactions (using '{current_type}'). must set tx type {}",
        bold(format!("-t {required_type}"))
    )]
    TxTypeInvalid {
        current_type: TxTypeCli,
        required_type: TxTypeCli,
    },

    #[error("failed to parse url")]
    UrlParse(#[from] url::ParseError),
}

#[derive(Debug, Error)]
pub enum SetupError {
    #[error("funding tx {0} failed: {1}")]
    FundingTxFailed(TxHash, PendingTransactionError),

    #[error(
        "funding tx {0} timed out after {1} seconds. This may indicate:\n\
            - Transaction stuck in mempool (try increasing gas price)\n\
            - Network congestion or RPC connectivity issues\n\
            - Transaction was dropped or replaced"
    )]
    FundingTxTimedOut(TxHash, u64),

    #[error("insufficient balance in provided user account(s): {:?}", broke_accounts
            .iter()
            .map(|(addr, bal)| format!("{}: {} ETH", addr, format_ether(*bal)))
            .collect::<Vec<_>>()
    )]
    InsufficientFunds {
        // accounts w/ balances
        broke_accounts: Vec<(Address, U256)>,
    },

    #[error("util error")]
    Util(#[from] UtilError),
}

impl ArgsError {
    pub fn engine_args_required(auth_rpc_url: Option<String>, jwt_secret: Option<PathBuf>) -> Self {
        Self::EngineArgsRequired {
            auth_rpc_url,
            jwt_secret,
        }
    }
}

impl SetupError {
    pub fn insufficient_funds(broke_accounts: Vec<(Address, U256)>) -> Self {
        Self::InsufficientFunds { broke_accounts }
    }
}
