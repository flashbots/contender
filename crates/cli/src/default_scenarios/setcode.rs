use contender_core::{
    error::ContenderError,
    generator::{types::SpamRequest, CreateDefinition, FunctionCallDefinition},
};
use contender_testfile::TestConfig;

use crate::{
    default_scenarios::{builtin::ToTestConfig, contracts::SPAM_ME_6},
    util::bold,
};

#[derive(Clone, Debug, clap::Parser)]
pub struct SetCodeCliArgs {
    /// The contract address containing the bytecode to copy into the sender's EOA.
    /// May be a placeholder. If not set, a test contract will be deployed.
    #[arg(
        long = "address",
        visible_aliases = ["contract-address"]
    )]
    pub contract_address: Option<String>,

    /// The solidity signature function to call on the EOA after the setCode transaction executes.
    #[arg(
        long = "sig",
        long_help = "The solidity signature function to call after setCode changes the account's bytecode. Must be provided with --address & --args",
        visible_aliases = ["signature"]
    )]
    pub signature: Option<String>,

    /// The arguments (comma-separated) to the function being called on the EOA after the setCode transaction executes.
    #[arg(
        long,
        long_help = "The solidity signature function to call after setCode changes the account's bytecode. Must be provided with --address & --sig",
        value_parser = clap::builder::ValueParser::new(|s: &str| {
            Ok::<_, String>(s.split(',')
            .map(|arg| arg.trim().to_owned())
            .filter(|arg| !arg.is_empty())
            .collect::<Vec<String>>())
        })
    )]
    pub args: Option<Vec<String>>,
}

#[derive(Clone, Debug)]
pub struct SetCodeArgs {
    pub contract_address: Option<String>,
    pub signature: String,
    pub args: Vec<String>,
}

impl SetCodeArgs {
    pub fn from_cli_args(cli_args: SetCodeCliArgs) -> contender_core::Result<Self> {
        if cli_args.contract_address.is_some() {
            // require signature & args to be provided, else error
            if cli_args.args.is_none() || cli_args.signature.is_none() {
                let addr_flag = bold("--address");
                let args_flag = bold("--args");
                let sig_flag = bold("--sig");
                return Err(ContenderError::SpamError(
                    "invalid arguments:",
                    Some(format!(
                        "both {args_flag} and {sig_flag} must be provided with {addr_flag}"
                    )),
                ));
            }
        }

        let signature = cli_args
            .signature
            .unwrap_or("consumeGas(uint256 amount)".to_owned());
        let args = cli_args.args.unwrap_or(vec!["21000".to_owned()]);

        Ok(Self {
            args,
            signature,
            // contract address remains optional so later, we know whether to deploy a new contract
            contract_address: cli_args.contract_address,
        })
    }
}

impl ToTestConfig for SetCodeArgs {
    fn to_testconfig(&self) -> contender_testfile::TestConfig {
        let fn_call = FunctionCallDefinition::new("{_sender}")
            .with_from_pool("spammers")
            .with_args(&self.args)
            .with_signature(self.signature.to_owned())
            .with_authorization(
                self.contract_address
                    .to_owned()
                    .unwrap_or(SPAM_ME_6.template_name()),
            );

        let spam = vec![fn_call].iter().map(SpamRequest::new_tx).collect();
        let mut config = TestConfig::new().with_spam(spam);

        // only add a create step if contract_address (already deployed) is NOT provided
        if self.contract_address.is_none() {
            config = config.with_create(vec![
                CreateDefinition::new(&SPAM_ME_6.into()).with_from_pool("admin")
            ]);
        }

        config
    }
}
