use crate::error::ContenderError;
use crate::Result;
use alloy::dyn_abi::{DynSolType, DynSolValue, JsonAbiExt};
use alloy::json_abi;
use alloy::rpc::types::TransactionRequest;
pub use seeder::rand_seed::RandSeed;

pub mod seeder;
pub mod testfile;
pub mod univ2;
pub mod util;

#[derive(Clone, Debug)]
pub struct NamedTxRequest {
    pub name: Option<String>,
    pub tx: TransactionRequest,
}

impl NamedTxRequest {
    pub fn with_name(name: &str, tx: TransactionRequest) -> Self {
        Self {
            name: Some(name.to_string()),
            tx,
        }
    }
}

impl From<TransactionRequest> for NamedTxRequest {
    fn from(tx: TransactionRequest) -> Self {
        Self { name: None, tx }
    }
}

/// Implement Generator to programmatically
/// generate transactions for advanced testing scenarios.
pub trait Generator {
    fn get_txs(&self, amount: usize) -> Result<Vec<NamedTxRequest>>;
    fn encode_calldata(&self, args: &[String], sig: &str) -> Result<Vec<u8>> {
        let func = json_abi::Function::parse(&sig).map_err(|e| {
            ContenderError::SpamError("failed to parse setup function name", Some(e.to_string()))
        })?;
        let values: Vec<DynSolValue> = args
            .iter()
            .enumerate()
            .map(|(idx, arg)| {
                let mut argtype = String::new();
                func.inputs[idx].full_selector_type_raw(&mut argtype);
                let r#type = DynSolType::parse(&argtype).map_err(|e| {
                    ContenderError::SpamError(
                        "failed to parse function signature",
                        Some(e.to_string()),
                    )
                })?;
                r#type.coerce_str(arg).map_err(|e| {
                    ContenderError::SpamError(
                        "failed to coerce args to function signature",
                        Some(e.to_string()),
                    )
                })
            })
            .collect::<Result<_>>()?;
        let input = func.abi_encode_input(&values).map_err(|e| {
            ContenderError::SpamError("failed to encode function arguments", Some(e.to_string()))
        })?;
        Ok(input)
    }
}
