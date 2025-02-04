use contender_core::generator::types::{CreateDefinition, FunctionCallDefinition, SpamRequest};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration to run a test scenario; used to generate PlanConfigs.
/// Defines TOML schema for scenario files.
#[derive(Clone, Deserialize, Debug, Serialize, Default)]
pub struct TestConfig {
    /// Template variables
    pub env: Option<HashMap<String, String>>,

    /// Contract deployments; array of hex-encoded bytecode strings.
    pub create: Option<Vec<CreateDefinition>>,

    /// Setup steps to run before spamming.
    pub setup: Option<Vec<FunctionCallDefinition>>,

    /// Function to call in spam txs.
    pub spam: Option<Vec<SpamRequest>>, // TODO: figure out how to implement BundleCallDefinition alongside FunctionCallDefinition
}
