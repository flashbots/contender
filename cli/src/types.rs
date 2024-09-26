use std::collections::HashMap;

use contender_core::generator::types::{CreateDefinition, FunctionCallDefinition};
use serde::{Deserialize, Serialize};

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
