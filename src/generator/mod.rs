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

pub struct MockGenerator;

impl Generator for MockGenerator {
    fn get_txs(&self, amount: usize) -> Result<Vec<NamedTxRequest>> {
        let mut txs = vec![];
        for _ in 0..amount {
            txs.push(NamedTxRequest::from(TransactionRequest::default()));
        }
        Ok(txs)
    }
}

/// Implement Generator to programmatically
/// generate transactions for advanced testing scenarios.
pub trait Generator {
    fn get_txs(&self, amount: usize) -> Result<Vec<NamedTxRequest>>;

    /// Encode the calldata for a function signature given an array of string arguments.
    ///
    /// ## Example
    /// ```
    /// use contender_core::generator::{Generator, MockGenerator};
    /// use alloy::hex::ToHexExt;
    ///
    /// let args = vec!["0x12345678"];
    /// let sig = "set(uint256 x)";
    /// let generator = MockGenerator; // you should use a real generator here
    /// let calldata = generator.encode_calldata(&args, sig).unwrap();
    /// assert_eq!(calldata.encode_hex(), "60fe47b10000000000000000000000000000000000000000000000000000000012345678");
    /// ```
    fn encode_calldata(&self, args: &[impl AsRef<str>], sig: &str) -> Result<Vec<u8>> {
        let func = json_abi::Function::parse(&sig).map_err(|e| {
            ContenderError::SpamError("failed to parse function name", Some(e.to_string()))
        })?;
        let values: Vec<DynSolValue> = args
            .iter()
            .enumerate()
            .map(|(idx, arg)| {
                let mut argtype = String::new();
                func.inputs[idx].full_selector_type_raw(&mut argtype);
                let r#type = DynSolType::parse(&argtype).map_err(|e| {
                    ContenderError::SpamError("failed to parse function type", Some(e.to_string()))
                })?;
                r#type.coerce_str(arg.as_ref()).map_err(|e| {
                    ContenderError::SpamError(
                        "failed to coerce arg to DynSolValue",
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
