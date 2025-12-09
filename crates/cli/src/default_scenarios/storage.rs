use crate::default_scenarios::{builtin::ToTestConfig, contracts};
use contender_core::generator::{types::SpamRequest, CreateDefinition, FunctionCallDefinition};
use contender_testfile::TestConfig;

#[derive(Debug, Clone, clap::Parser)]
pub struct StorageStressCliArgs {
    #[arg(
        short = 's',
        long = "storage.num-slots",
        default_value_t = 500,
        help = "Number of storage slots to fill with random data."
    )]
    pub num_slots: u64,

    #[arg(
        short,
        long = "storage.num-writes",
        default_value_t = 1,
        help = "Number of times to write over each storage slot."
    )]
    pub num_iterations: u64,
}

impl Default for StorageStressCliArgs {
    fn default() -> Self {
        Self {
            num_slots: 500,
            num_iterations: 1,
        }
    }
}

#[derive(Clone, Debug)]
pub struct StorageStressArgs {
    pub num_slots: u64,
    pub num_iterations: u64,
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
            FunctionCallDefinition::new(contracts::SPAM_ME.template_name())
                .with_signature("fillStorageSlots(uint256 numSlots, uint256 iteration)")
                .with_args(&[num_slots.to_string(), num_iterations.to_string()]),
            // ... add more transactions here if needed.
        ]
        .into_iter()
        .map(Box::new)
        .map(SpamRequest::Tx)
        .collect::<Vec<_>>();

        TestConfig {
            env: None,
            create: Some(vec![CreateDefinition {
                contract: contracts::SPAM_ME.into(),
                signature: None,
                args: None,
                from: None,
                from_pool: Some("admin".to_owned()),
            }]),
            setup: None,
            spam: Some(txs),
        }
    }
}
