use contender_core::generator::{types::SpamRequest, CreateDefinition, FunctionCallDefinition};
use contender_testfile::TestConfig;

use crate::default_scenarios::{builtin::ToTestConfig, contracts::SPAM_ME_6};

#[derive(Clone, Debug, clap::Parser)]
pub struct SetCodeCliArgs {
    /// The contract address containing the bytecode to copy into the sender's EOA.
    /// May be a placeholder. If not set, a test contract will be deployed.
    #[arg(
        short = 'a',
        long,
        visible_aliases = &["address"]
    )]
    pub contract_address: Option<String>,

    /// The function call to execute on the EOA after setCode changes the account's bytecode.
    /// If not provided, `consumeGas(21000)` will be called on the test contract.
    #[arg(long)]
    pub call: Option<String>,
}

impl ToTestConfig for SetCodeCliArgs {
    fn to_testconfig(&self) -> contender_testfile::TestConfig {
        // TODO: not here, but somewhere that returns a Result: warn user if they provide funky args
        // e.g. providing a function call without

        let spam = vec![FunctionCallDefinition::new("{_sender}")
            .with_from_pool("spammers")
            // TODO: parse signature and args from `call` correctly
            // we need the ABI....... how tf are we gonna get that if we didn't compile it ourselves?
            // we can't have the ABI so we need to change the format of the `call` input...
            // .with_signature(
            //     self.call
            //         .to_owned()
            //         .unwrap_or("consumeGas(uint256 amount)".to_owned()),
            // )
            // .with_args(args)
            .with_authorization(
                self.contract_address
                    .to_owned()
                    .unwrap_or(SPAM_ME_6.template_name()),
            )]
        .iter()
        .map(SpamRequest::new_tx)
        .collect();
        let mut config = TestConfig::new().with_spam(spam);

        if self.contract_address.is_none() {
            config = config.with_create(vec![CreateDefinition::new(&SPAM_ME_6.into())]);
        }

        config
    }
}
