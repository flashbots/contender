use contender_core::generator::{types::SpamRequest, CreateDefinition, FunctionCallDefinition};
use contender_testfile::TestConfig;
use tracing::warn;

use crate::{
    default_scenarios::{
        builtin::ToTestConfig,
        contracts::{COUNTER, SMART_WALLET},
    },
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
    /// setCode txs must be sent to this address bc it's the signer on the `Authorization`.
    signer_address: String,
}

impl SetCodeArgs {
    pub fn from_cli_args(
        cli_args: SetCodeCliArgs,
        signer_address: String,
    ) -> contender_core::Result<Self> {
        const DEFAULT_SIG: &str = "execute((address,uint256,bytes)[])";
        const DEFAULT_ARGS: &str = "[(0x{Counter},0,0xd09de08a)]";
        if cli_args.contract_address.is_some() {
            // require signature & args to be provided, else error
            if cli_args.args.is_none() || cli_args.signature.is_none() {
                warn!(
                    "No args or signature provided, using defaults: {}",
                    bold(format!("--sig \"{DEFAULT_SIG}\" --args \"{DEFAULT_ARGS}\""))
                )
            }
        }

        // 0xd09de08a is the function signature for `increment()` (which we'll call on the Counter contract)
        let signature = cli_args.signature.unwrap_or(DEFAULT_SIG.to_owned());
        let args = cli_args.args.unwrap_or(vec![DEFAULT_ARGS.to_owned()]);

        Ok(Self {
            args,
            signature,
            // contract address remains optional so later, we know whether to deploy a new contract
            contract_address: cli_args.contract_address,
            signer_address,
        })
    }
}

impl ToTestConfig for SetCodeArgs {
    fn to_testconfig(&self) -> contender_testfile::TestConfig {
        let fn_call = FunctionCallDefinition::new(&self.signer_address)
            .with_from_pool("spammers")
            .with_args(&self.args)
            .with_signature(&self.signature)
            .with_authorization(
                self.contract_address
                    .to_owned()
                    .unwrap_or(SMART_WALLET.template_name()),
            );

        let spam = vec![fn_call].iter().map(SpamRequest::new_tx).collect();
        let mut config = TestConfig::new().with_spam(spam);

        // add default create steps if contract_address (must be already deployed) is NOT provided
        if self.contract_address.is_none() {
            config = config.with_create(
                [COUNTER, SMART_WALLET]
                    .into_iter()
                    .map(|contract| CreateDefinition::new(&contract.into()).with_from_pool("admin"))
                    .collect(),
            );
        }

        config
    }
}
