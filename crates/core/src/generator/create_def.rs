use alloy::{hex::ToHexExt, primitives::Address};
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::generator::{error::GeneratorError, util::encode_calldata};

#[derive(Clone, Deserialize, Debug, Serialize)]
pub struct CompiledContract<S: AsRef<str> = String> {
    pub bytecode: S,
    pub name: S,
}

impl<T: AsRef<str>> CompiledContract<T> {
    pub fn new(bytecode: T, name: T) -> Self {
        CompiledContract { bytecode, name }
    }

    /// Returns the contract name as a template string (wrapped in curly braces).
    pub fn template_name(&self) -> String {
        format!("{{{}}}", self.name.as_ref())
    }
}

impl<'a> From<CompiledContract<&'a str>> for CompiledContract<String> {
    fn from(contract: CompiledContract<&'a str>) -> Self {
        CompiledContract {
            bytecode: contract.bytecode.to_string(),
            name: contract.name.to_string(),
        }
    }
}

impl CompiledContract {
    pub fn with_constructor_args(
        mut self,
        sig: impl AsRef<str>,
        args: &[impl AsRef<str>],
    ) -> Result<Self, GeneratorError> {
        let sig = sig.as_ref();
        let sig = if sig.starts_with("(") {
            // coerce sig into `function(...)` format to make encode_calldata happy
            &format!("constructor{sig}")
        } else {
            sig
        };

        // encode calldata
        let calldata = encode_calldata(args, sig)?;

        // remove function selector from calldata
        let calldata = if calldata.len() >= 4 {
            calldata[4..].to_vec()
        } else {
            Vec::new()
        };
        debug!("calldata (no selector): {}", calldata.encode_hex());

        self.bytecode.push_str(&calldata.encode_hex());

        Ok(self)
    }
}

#[derive(Clone, Deserialize, Debug, Serialize)]
pub struct CreateDefinition {
    #[serde(flatten)]
    pub contract: CompiledContract,
    /// Constructor signature. Formats supported: "constructor(type1,type2,...)" or "(type1,type2,...)".
    pub signature: Option<String>,
    /// Constructor arguments. May include placeholders.
    pub args: Option<Vec<String>>,
    /// Address of the tx sender.
    pub from: Option<String>,
    /// Get a `from` address from the pool of signers specified here.
    pub from_pool: Option<String>,
}

impl CreateDefinition {
    pub fn new(contract: &CompiledContract) -> Self {
        CreateDefinition {
            contract: contract.to_owned(),
            signature: None,
            args: None,
            from: None,
            from_pool: None,
        }
    }

    pub fn with_from_pool(mut self, from_pool: impl AsRef<str>) -> Self {
        self.from_pool = Some(from_pool.as_ref().to_owned());
        self
    }

    pub fn with_from(mut self, from: impl AsRef<str>) -> Self {
        self.from = Some(from.as_ref().to_owned());
        self
    }
}

pub struct CreateDefinitionStrict {
    pub bytecode: String,
    pub name: String,
    pub from: Address,
    pub signature: Option<String>,
    pub args: Vec<String>,
}
