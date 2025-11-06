use alloy::primitives::{Address, U256};
use contender_core::generator::{types::SpamRequest, CreateDefinition, FunctionCallDefinition, FuzzParam};
use contender_testfile::TestConfig;
use std::str::FromStr;

use crate::{
    commands::common::parse_amount,
    default_scenarios::{builtin::ToTestConfig, contracts::test_token},
};

#[derive(Clone, Debug, clap::Parser)]
pub struct Erc20CliArgs {
    #[arg(
        short,
        long,
        long_help = "The amount to send in each spam tx.",
        default_value = "0.00001 ether",
        value_parser = parse_amount,
    )]
    pub send_amount: U256,

    #[arg(
        short,
        long,
        long_help = "The amount of tokens to give each spammer account before spamming starts.",
        default_value = "1000000 ether",
        value_parser = parse_amount,
    )]
    pub fund_amount: U256,

    #[arg(
        short = 'r',
        long = "recipient",
        long_help = "The address to receive tokens sent by spam txs. By default, the sender receives their own tokens."
    )]
    pub token_recipient: Option<Address>,
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
            spam: Some(vec![SpamRequest::new_tx(
                &FunctionCallDefinition::new(token.template_name())
                    .with_from_pool("spammers")  // Senders from limited pool
                    .with_signature("transfer(address guy, uint256 wad)")
                    .with_args(&[
                        // This will be overwritten by the fuzzer below
                        "0x0000000000000000000000000000000000000000",
                        &self.send_amount.to_string(),
                    ])
                    .with_gas_limit(55000)
                    .with_fuzz(&[FuzzParam {
                        param: Some("guy".to_string()),  // Must match the param name in signature
                        value: None,
                        min: Some(U256::from(1)),  // Avoid zero address
                        max: Some(U256::from_str(
                            "0x00ffffffffffffffffffffffffffffffffffffffff"  // Max valid address
                        ).unwrap()),
                    }]),
            )]),
        }
    }
}
