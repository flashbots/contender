use std::str::FromStr;

use alloy::{json_abi, primitives::Address};
use thiserror::Error;

use crate::generator::{templater::TemplaterError, util::UtilError};

#[derive(Debug, Error)]
pub enum GeneratorError {
    #[error("abi parser error")]
    AbiParserFailed(#[from] json_abi::parser::Error),

    #[error("could not find address '{0}' in placeholder map")]
    AddressNotFound(String),

    #[error("failed to parse blob data; invalid hex: {0}")]
    BlobDataParseFailed(String),

    #[error("failed to parse 'from' address '{from}': {error}")]
    FromAddressParseFailed {
        from: String,
        error: <Address as FromStr>::Err,
    },

    #[error("from_pool {0} not found in agent store")]
    FromPoolNotFound(String),

    #[error("fuzz must specify either `param` or `value`")]
    FuzzMissingParams,

    #[error("fuzz.value is false, but no param is specified")]
    FuzzValueNeedsParam,

    #[error("fuzz cannot specify both `param` and `value`; choose one per fuzz directive")]
    FuzzConflictingParams,

    #[error("fuzz invalid")]
    FuzzInvalid,

    #[error("must specify from or from_pool in scenario config")]
    InvalidSender,

    #[error("failed to find nonce for address '{0}'")]
    NonceNotFound(Address),

    #[error("failed to build sidecar")]
    SidecarBuildFailed,

    #[error("signer not found in agent store: from_pool={from_pool}, idx={idx}")]
    SignerNotFound { from_pool: String, idx: usize },

    #[error("templater error")]
    Templater(#[from] TemplaterError),

    #[error("generator util error")]
    Util(#[from] UtilError),
}

impl GeneratorError {
    pub fn address_not_found(addr: impl ToString) -> Self {
        Self::AddressNotFound(addr.to_string())
    }

    pub fn signer_not_found(from_pool: impl ToString, idx: usize) -> Self {
        Self::SignerNotFound {
            from_pool: from_pool.to_string(),
            idx,
        }
    }

    pub fn from_address_parse_failed(
        from: impl ToString,
        error: <Address as FromStr>::Err,
    ) -> Self {
        Self::FromAddressParseFailed {
            from: from.to_string(),
            error,
        }
    }

    pub fn from_pool_not_found(from: impl ToString) -> Self {
        Self::FromPoolNotFound(from.to_string())
    }
}
