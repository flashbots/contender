use alloy::primitives::{Address, U256};
use contender_core::generator::{
    types::SpamRequest, util::parse_value, CreateDefinition, FunctionCallDefinition, FuzzParam,
};
use contender_testfile::TestConfig;
use std::str::FromStr;

use crate::default_scenarios::{builtin::ToTestConfig, contracts::test_token};

#[derive(Clone, Debug, clap::Parser)]
pub struct Erc20CliArgs {
    #[arg(
        short,
        long,
        long_help = "The amount to send in each spam tx.",
        default_value = "0.00001 ether",
        value_parser = parse_value,
    )]
    pub send_amount: U256,

    #[arg(
        short,
        long,
        long_help = "The amount of tokens to give each spammer account before spamming starts.",
        default_value = "1000000 ether",
        value_parser = parse_value,
    )]
    pub fund_amount: U256,

    #[arg(
        short = 'r',
        long = "recipient",
        long_help = "The address to receive tokens sent by spam txs. By default, address(0) receives the tokens."
    )]
    pub token_recipient: Option<Address>,
}

impl Default for Erc20CliArgs {
    fn default() -> Self {
        Self {
            // Match clap default_value: "0.00001 ether"
            send_amount: parse_value("0.00001 ether").expect("valid default send_amount"),
            // Match clap default_value: "1000000 ether"
            fund_amount: parse_value("1000000 ether").expect("valid default fund_amount"),
            token_recipient: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Erc20Args {
    pub send_amount: U256,
    pub fund_amount: U256,
    /// populated by AgentStore for setup step
    pub fund_recipients: Vec<Address>,
    /// given by user to override token recipient
    pub token_recipient: Option<String>,
}

impl Erc20Args {
    pub fn from_cli_args(args: Erc20CliArgs, fund_recipients: &[Address]) -> Self {
        Erc20Args {
            fund_amount: args.fund_amount,
            fund_recipients: fund_recipients.to_vec(),
            send_amount: args.send_amount,
            token_recipient: args.token_recipient.map(|addr| addr.to_string()),
        }
    }
}

impl ToTestConfig for Erc20Args {
    fn to_testconfig(&self) -> contender_testfile::TestConfig {
        let token = test_token(0, U256::MAX);
        // transfer eth from admin (total supply is minted to that account) to spammers
        let setup_steps = self
            .fund_recipients
            .iter()
            .map(|recipient| {
                FunctionCallDefinition::new(token.template_name())
                    .with_signature("transfer(address guy, uint256 wad)")
                    .with_args(&[recipient.to_string(), self.fund_amount.to_string()])
            })
            .collect();

        TestConfig {
            env: None,
            create: Some(vec![CreateDefinition {
                contract: token.to_owned(),
                signature: None,
                args: None,
                from: None,
                from_pool: Some("admin".to_owned()),
            }]),
            setup: Some(setup_steps),
            spam: Some(vec![SpamRequest::new_tx(&{
                let mut func_def = FunctionCallDefinition::new(token.template_name())
                    .with_from_pool("spammers") // Senders from limited pool
                    .with_signature("transfer(address guy, uint256 wad)")
                    .with_args(&[
                        // Use token_recipient if provided (via --recipient flag),
                        // otherwise this is a placeholder for fuzzing
                        self.token_recipient
                            .as_ref()
                            .map(|addr| addr.to_string())
                            .unwrap_or_else(|| {
                                "0x0000000000000000000000000000000000000000".to_string()
                            }),
                        self.send_amount.to_string(),
                    ])
                    .with_gas_limit(55000);

                // Only add fuzzing if token_recipient is NOT provided
                if self.token_recipient.is_none() {
                    func_def = func_def.with_fuzz(&[FuzzParam {
                        param: Some("guy".to_string()),
                        value: None,
                        min: Some(U256::from(1)),
                        max: Some(
                            U256::from_str("0x0000000000ffffffffffffffffffffffffffffffff").unwrap(),
                        ),
                    }]);
                }

                func_def
            })]),
        }
    }
}
