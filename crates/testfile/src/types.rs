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

impl TestConfig {
    pub fn get_create_pools(&self) -> Vec<String> {
        self.create
            .to_owned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|s| s.from_pool)
            .collect()
    }

    pub fn get_setup_pools(&self) -> Vec<String> {
        self.setup
            .to_owned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|s| s.from_pool)
            .collect()
    }

    /// Gets every instance of a `from_pool` declaration in the spam requests.
    pub fn get_spam_pools(&self) -> Vec<String> {
        let mut from_pools = vec![];
        let spam = self
            .spam
            .as_ref()
            .expect("No spam function calls found in testfile");

        for s in spam {
            match s {
                SpamRequest::Tx(fn_call) => {
                    if let Some(from_pool) = &fn_call.from_pool {
                        from_pools.push(from_pool.to_owned());
                    }
                }
                SpamRequest::Bundle(bundle) => {
                    for tx in &bundle.txs {
                        if let Some(from_pool) = &tx.from_pool {
                            from_pools.push(from_pool.to_owned());
                        }
                    }
                }
            }
        }

        // filter out non-unique pools
        from_pools.sort();
        from_pools.dedup();
        from_pools
    }
}
