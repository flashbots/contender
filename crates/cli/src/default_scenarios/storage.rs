use contender_core::generator::types::{CreateDefinition, FunctionCallDefinition, SpamRequest};
use contender_testfile::TestConfig;
use serde::{Deserialize, Serialize};

use crate::default_scenarios::{builtin::ToTestConfig, contracts};

#[derive(Debug, Clone, clap::Parser, Deserialize, Serialize)]
pub struct StorageStressCliArgs {
    #[arg(
        short = 's',
        long = "num-slots",
        default_value_t = 1000,
        help = "Number of storage slots to fill with random data."
    )]
    pub num_slots: u64,
    #[arg(
        short,
        long = "num-iterations",
        default_value_t = 1,
        help = "Number of times to write over each storage slot."
    )]
    pub num_iterations: u64,
}

#[derive(Clone, Debug)]
pub struct StorageStressArgs {
    num_slots: u64,
    num_iterations: u64,
}

impl From<StorageStressCliArgs> for StorageStressArgs {
    fn from(args: StorageStressCliArgs) -> Self {
        StorageStressArgs {
            num_slots: args.num_slots,
            num_iterations: args.num_iterations,
        }
    }
}

impl ToTestConfig for StorageStressArgs {
    fn to_testconfig(&self) -> TestConfig {
        let StorageStressArgs {
            num_slots,
            num_iterations,
        } = self;
        let txs = [
            FunctionCallDefinition::new(
                contracts::SPAM_ME.template_name(),
                Some("fillStorageSlots(uint256 numSlots, uint256 iteration)"),
            )
            .with_args(&[num_slots.to_string(), num_iterations.to_string()])
            .with_from_pool("admin"),
            // ... add more transactions here if needed.
        ]
        .into_iter()
        .map(SpamRequest::Tx)
        .collect::<Vec<_>>();

        TestConfig {
            env: None,
            create: Some(vec![CreateDefinition {
                contract: contracts::SPAM_ME.into(),
                from: None,
                from_pool: Some("admin".to_owned()),
            }]),
            setup: None,
            spam: Some(txs),
        }
    }
}
