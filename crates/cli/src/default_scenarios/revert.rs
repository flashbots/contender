use contender_core::generator::{types::SpamRequest, CreateDefinition, FunctionCallDefinition};
use contender_testfile::TestConfig;

use crate::default_scenarios::{builtin::ToTestConfig, contracts::SPAM_ME_6};

#[derive(Clone, Debug, clap::Parser)]
pub struct RevertCliArgs {
    /// Amount of gas to use before reverting.
    #[arg(
        short,
        long,
        long_help = "Amount of gas to use before reverting. This number + 35k gas is added to each tx's gas limit.",
        default_value_t = 30_000
    )]
    pub gas_use: u64,
}

impl ToTestConfig for RevertCliArgs {
    fn to_testconfig(&self) -> contender_testfile::TestConfig {
        TestConfig {
            env: None,
            create: Some(vec![CreateDefinition {
                contract: SPAM_ME_6.into(),
                signature: None,
                args: None,
                from: None,
                from_pool: Some("admin".to_owned()),
            }]),
            setup: None,
            spam: Some(vec![SpamRequest::new_tx(
                &FunctionCallDefinition::new(SPAM_ME_6.template_name())
                    .with_from_pool("spammers")
                    .with_signature("consumeGasAndRevert(uint256 gas)")
                    .with_args(&[self.gas_use.to_string()])
                    .with_gas_limit(self.gas_use + 35000),
            )]),
        }
    }
}
