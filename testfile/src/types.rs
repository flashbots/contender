use contender_core::generator::types::{CreateDefinition, FunctionCallDefinition};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

impl Default for TestConfig {
    fn default() -> Self {
        TestConfig {
            env: HashMap::new().into(),
            create: vec![].into(),
            setup: vec![].into(),
            spam: vec![].into(),
        }
    }
}
