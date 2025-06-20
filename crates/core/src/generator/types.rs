use super::named_txs::ExecutionRequest;
use alloy::{
    network::AnyNetwork,
    primitives::{Address, U256},
    providers::DynProvider,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::task::JoinHandle;

// -- re-exports
pub use crate::generator::named_txs::NamedTxRequest;

// -- convenience
pub type AnyProvider = DynProvider<AnyNetwork>;

// -- core types for test scenarios

/// User-facing definition of a function call to be executed.
#[derive(Clone, Deserialize, Debug, Serialize)]
pub struct FunctionCallDefinition {
    /// Address of the contract to call.
    pub to: String,
    /// Address of the tx sender.
    pub from: Option<String>,
    /// Get a `from` address from the pool of signers specified here.
    pub from_pool: Option<String>,
    /// Name of the function to call.
    pub signature: Option<String>,
    /// Parameters to pass to the function.
    pub args: Option<Vec<String>>,
    /// Value in wei to send with the tx.
    pub value: Option<String>,
    /// Parameters to fuzz during the test.
    pub fuzz: Option<Vec<FuzzParam>>,
    /// Optional type of the spam transaction for categorization.
    pub kind: Option<String>,
    /// Optional gas limit, which will skip gas estimation. This allows reverting txs to be sent.
    pub gas_limit: Option<u64>,
}

impl FunctionCallDefinition {
    pub fn new(to: impl AsRef<str>, signature: Option<&str>) -> Self {
        FunctionCallDefinition {
            to: to.as_ref().to_owned(),
            from: None,
            from_pool: None,
            signature: signature.map(|s| s.to_owned()),
            args: None,
            value: None,
            fuzz: None,
            kind: None,
            gas_limit: None,
        }
    }

    pub fn with_from(mut self, from: impl AsRef<str>) -> Self {
        self.from = Some(from.as_ref().to_owned());
        self
    }
    pub fn with_from_pool(mut self, from_pool: impl AsRef<str>) -> Self {
        self.from_pool = Some(from_pool.as_ref().to_owned());
        self
    }
    pub fn with_args(mut self, args: &[impl AsRef<str>]) -> Self {
        self.args = Some(
            args.iter()
                .map(|t| t.as_ref().to_owned())
                .collect::<Vec<_>>(),
        );
        self
    }
    /// Set value in wei to send with the tx.
    pub fn with_value(mut self, value: U256) -> Self {
        self.value = Some(value.to_string());
        self
    }
    pub fn with_fuzz(mut self, fuzz: &[FuzzParam]) -> Self {
        self.fuzz = Some(fuzz.to_vec());
        self
    }
    pub fn with_kind(mut self, kind: impl AsRef<str>) -> Self {
        self.kind = Some(kind.as_ref().to_owned());
        self
    }
    pub fn with_gas_limit(mut self, gas_limit: u64) -> Self {
        self.gas_limit = Some(gas_limit);
        self
    }
}

pub struct FunctionCallDefinitionStrict {
    pub to: String, // may be a placeholder, so we can't use Address
    pub from: Address,
    pub signature: String,
    pub args: Vec<String>,
    pub value: Option<String>,
    pub fuzz: Vec<FuzzParam>,
    pub kind: Option<String>,
    pub gas_limit: Option<u64>,
}

/// User-facing definition of a function call to be executed.
#[derive(Clone, Deserialize, Debug, Serialize)]
pub struct BundleCallDefinition {
    #[serde(rename = "tx")]
    pub txs: Vec<FunctionCallDefinition>,
}

/// Definition of a spam request template.
/// TestConfig uses this for TOML parsing.
#[derive(Clone, Deserialize, Debug, Serialize)]
pub enum SpamRequest {
    #[serde(rename = "tx")]
    Tx(FunctionCallDefinition),
    #[serde(rename = "bundle")]
    Bundle(BundleCallDefinition),
}

impl SpamRequest {
    pub fn is_bundle(&self) -> bool {
        matches!(self, SpamRequest::Bundle(_))
    }
}

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

#[derive(Clone, Deserialize, Debug, Serialize)]
pub struct FuzzParam {
    /// Name of the parameter to fuzz.
    pub param: Option<String>,
    /// Fuzz the `value` field of the tx (ETH sent with the tx).
    pub value: Option<bool>,
    /// Minimum value fuzzer will use.
    pub min: Option<U256>,
    /// Maximum value fuzzer will use.
    pub max: Option<U256>,
}

#[derive(Debug)]
pub struct Plan {
    pub env: HashMap<String, String>,
    pub create_steps: Vec<NamedTxRequest>,
    pub setup_steps: Vec<NamedTxRequest>,
    pub spam_steps: Vec<ExecutionRequest>,
}

pub type CallbackResult = crate::Result<Option<JoinHandle<crate::Result<()>>>>;

/// Defines the type of plan to be executed.
pub enum PlanType<F: Fn(NamedTxRequest) -> CallbackResult> {
    /// Run contract deployments, triggering a callback after each tx is processed.
    Create(F),
    /// Run setup steps, triggering a callback after each tx is processed.
    Setup(F),
    /// Spam with a number of txs and trigger a callback after each one is processed.
    Spam(u64, F),
}
