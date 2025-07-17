use super::named_txs::ExecutionRequest;
use alloy::{network::AnyNetwork, providers::DynProvider};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::task::JoinHandle;

use crate::generator::{function_def::FunctionCallDefinition, BundleCallDefinition};
// -- re-exports
pub use crate::generator::named_txs::NamedTxRequest;

// -- convenience
pub type AnyProvider = DynProvider<AnyNetwork>;

// -- core types for test scenarios
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
