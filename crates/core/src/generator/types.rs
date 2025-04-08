use super::named_txs::ExecutionRequest;
use alloy::{
    network::{AnyNetwork, Ethereum},
    primitives::{Address, U256},
    providers::DynProvider,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::task::JoinHandle;

// -- re-exports
pub use crate::generator::named_txs::NamedTxRequest;

// -- convenience
pub type EthProvider = DynProvider<Ethereum>;
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
    pub signature: String,
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

#[derive(Clone, Deserialize, Debug, Serialize)]
pub struct CreateDefinition {
    /// Bytecode of the contract to deploy.
    pub bytecode: String,
    /// Name to identify the contract later.
    pub name: String,
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

pub type CallbackResult = crate::Result<Option<JoinHandle<()>>>;

/// Defines the type of plan to be executed.
pub enum PlanType<F: Fn(NamedTxRequest) -> CallbackResult> {
    /// Run contract deployments, triggering a callback after each tx is sent.
    Create(F),
    /// Run setup steps, triggering a callback after each tx is sent.
    Setup(F),
    /// Spam with a number of txs and trigger a callback after each one is sent.
    Spam(usize, F),
}
