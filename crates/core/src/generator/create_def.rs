use alloy::primitives::Address;
use serde::{Deserialize, Serialize};

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

#[derive(Clone, Deserialize, Debug, Serialize)]
pub struct CreateDefinition {
    #[serde(flatten)]
    pub contract: CompiledContract,
    /// Address of the tx sender.
    pub from: Option<String>,
    /// Get a `from` address from the pool of signers specified here.
    pub from_pool: Option<String>,
}

pub struct CreateDefinitionStrict {
    pub bytecode: String,
    pub name: String,
    pub from: Address,
}
