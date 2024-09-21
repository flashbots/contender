use alloy::{
    primitives::U256,
    providers::RootProvider,
    rpc::types::TransactionRequest,
    transports::http::{Client, Http},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::task::JoinHandle;

pub type RpcProvider = RootProvider<Http<Client>>;

#[derive(Clone, Debug)]
pub struct NamedTxRequest {
    pub name: Option<String>,
    pub tx: TransactionRequest,
}

/// Configuration to run a test scenario; used to generate PlanConfigs.
#[derive(Clone, Deserialize, Debug, Serialize)]
pub struct TestConfig {
    /// Template variables
    pub env: Option<HashMap<String, String>>,

    /// Contract deployments; array of hex-encoded bytecode strings.
    pub create: Option<Vec<CreateDefinition>>,

    /// Setup steps to run before spamming.
    pub setup: Option<Vec<FunctionCallDefinition>>,

    /// Function to call in spam txs.
    pub spam: Option<Vec<FunctionCallDefinition>>,
}

#[derive(Clone, Deserialize, Debug, Serialize)]
pub struct FunctionCallDefinition {
    /// Address of the contract to call.
    pub to: String,
    /// Address of the tx sender.
    pub from: String,
    /// Name of the function to call.
    pub signature: String,
    /// Parameters to pass to the function.
    pub args: Option<Vec<String>>,
    /// Value in wei to send with the tx.
    pub value: Option<String>,
    /// Parameters to fuzz during the test.
    pub fuzz: Option<Vec<FuzzParam>>,
}

#[derive(Clone, Deserialize, Debug, Serialize)]
pub struct CreateDefinition {
    /// Bytecode of the contract to deploy.
    pub bytecode: String,
    /// Name to identify the contract later.
    pub name: String,
    /// Address of the tx sender.
    pub from: String,
}

#[derive(Clone, Deserialize, Debug, Serialize)]
pub struct FuzzParam {
    /// Name of the parameter to fuzz.
    pub param: String,
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
    pub spam_steps: Vec<NamedTxRequest>,
}

pub type CallbackResult = crate::Result<Option<JoinHandle<()>>>;

pub enum PlanType<F: Fn(NamedTxRequest) -> CallbackResult> {
    Create(F),
    Setup(F),
    Spam(usize, F),
}
