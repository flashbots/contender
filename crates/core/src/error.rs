use crate::{db::DbError, generator::NamedTxRequest, spammer::CallbackError};
use alloy::{
    dyn_abi, json_abi,
    network::{Ethereum, TransactionBuilderError},
    primitives::Address,
    providers::PendingTransactionError,
    rpc::types::TransactionRequest,
    signers,
    transports::{RpcError, TransportErrorKind},
};
use contender_bundle_provider::error::BundleProviderError;
use contender_engine_provider::AuthProviderError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("auth provider error")]
    AuthProvider(#[from] AuthProviderError),

    #[error("failed to encode function args")]
    DynAbi(#[from] dyn_abi::Error),

    #[error("abi parser error")]
    AbiParser(#[from] json_abi::parser::Error),

    #[error("critical error from callback")]
    Callback(#[from] CallbackError),

    #[error("invalid configuration: {0}")]
    Config(String), // TODO: replace String with ConfigError

    #[error("database error")]
    Db(#[from] DbError),

    #[error("generator error: {0}")]
    Generator(String), // TODO: replace String with GeneratorError

    #[error("rpc error")]
    Rpc(#[from] RpcError<TransportErrorKind>),

    #[error("internal rpc error: {0}")]
    RpcInternal(RpcErrorKind),

    #[error("failed to find pending tx")]
    PendingTx(#[from] PendingTransactionError),

    #[error("runtime error")]
    Runtime(#[from] RuntimeErrorKind),

    #[error("failed to build eth transaction")]
    TransactionBuilderEth(#[from] TransactionBuilderError<Ethereum>),

    #[error("signer failed to sign hash")]
    Signer(#[from] signers::Error),

    #[error("templater error: {0}")]
    Templater(String),
}

#[derive(Debug, Error)]
pub enum RuntimeErrorKind {
    #[error("failed to spawn anvil. You may need to install foundry (https://book.getfoundry.sh/getting-started/installation).")]
    AnvilMissing,

    #[error("anvil failed to become ready within timeout, waited {0} seconds")]
    AnvilTimeout(u64),

    #[error("anvil error: {0}")]
    AnvilUnchecked(String),

    #[error("chain_id mismatch. primary chain id: {0}, builder chain id: {1}. chain_id must be consistent across rpc and builder")]
    ChainIdMismatch(u64, u64),

    #[error("no gas limit was set for tx {0:?}")]
    GasLimitMissingFromMap(TransactionRequest),

    #[error("no genesis block found")]
    GenesisBlockMissing,

    #[error("NamedTxRequest requires a 'from' address: {0:?}")]
    NamedTxMissingFromAddress(NamedTxRequest),

    #[error("couldn't find nonce for 'from' address {0}")]
    NonceMissing(Address),

    #[error("couldn't find private key for address {0}")]
    PrivateKeyMissing(Address),

    #[error("failed to get signer for {0}")]
    SignerMissingFromMap(Address),

    #[error("cannot proceed; there are no spam txs")]
    SpamTxsEmpty,

    #[error("tx request requires a 'from' address: {0:?}")]
    TxMissingFromAddress(TransactionRequest),

    #[error("invalid runtime params")]
    InvalidParams(#[from] RuntimeParamErrorKind),
}

impl From<alloy::node_bindings::NodeError> for Error {
    fn from(e: alloy::node_bindings::NodeError) -> Self {
        if e.to_string().to_lowercase().contains("no such file") {
            RuntimeErrorKind::AnvilMissing.into()
        } else {
            RuntimeErrorKind::AnvilUnchecked(e.to_string()).into()
        }
    }
}

/// Wrapper for common errors that we can work around at runtime.
#[derive(Debug)]
pub enum RpcErrorKind {
    TxAlreadyKnown,
    InsufficientFunds(Address),
    ReplacementTransactionUnderpriced,
    GenericSendTxError,
}

/// Wrapper for common errors that we can work around at runtime.
#[derive(Debug, Error)]
pub enum RuntimeParamErrorKind {
    #[error("builder URL is required")]
    BuilderUrlRequired,

    #[error("invalid builder URL")]
    BuilderUrlInvalid,

    #[error("invalid bundle type.")]
    BundleTypeInvalid,

    #[error("invalid arg(s): '{0}'")]
    InvalidArgs(String),

    #[error("missing required arg(s): {0}")]
    MissingArgs(String),
}

impl std::fmt::Display for RpcErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        use RpcErrorKind::*;
        match self {
            TxAlreadyKnown => write!(f, "Transaction already known. You may be using the same seed (or private key) as another spammer."),
            InsufficientFunds(address) => write!(f, "Insufficient funds for transaction (from {address})."),
            ReplacementTransactionUnderpriced => {
                write!(f, "Replacement transaction underpriced. You may have to wait, or replace the currently-pending transactions manually.")
            }
            GenericSendTxError => write!(f, "Failed to send transaction. This may be due to a network issue or the transaction being invalid."),
        }
    }
}

impl From<BundleProviderError> for Error {
    fn from(err: BundleProviderError) -> Error {
        use Error::Runtime;
        match err {
            BundleProviderError::InvalidUrl => {
                Runtime(RuntimeParamErrorKind::BuilderUrlInvalid.into())
            }
            BundleProviderError::SendBundleError(e) => {
                if e.to_string()
                    .contains("bundle must contain exactly one transaction")
                {
                    return Runtime(RuntimeParamErrorKind::BundleTypeInvalid.into());
                }
                Error::Rpc(e)
            }
        }
    }
}
