use crate::{commands::common::parse_amount, default_scenarios::builtin::ToTestConfig};
use alloy::primitives::{Address, U256};
use clap::{arg, Parser};
use contender_core::generator::{types::SpamRequest, FunctionCallDefinition};

#[derive(Parser, Clone, Debug)]
pub struct TransferStressCliArgs {
    #[arg(
        short = 'a',
        long = "transfer.amount",
        visible_aliases = &["ta", "amount"],
        default_value = "0.001 eth",
        value_parser = parse_amount,
        help = "Amount of tokens to transfer in each transaction."
    )]
    pub amount: U256,
    #[arg(
        short,
        long = "transfer.recipient",
        visible_aliases = &["tr", "recipient"],
        help = "Address to receive ether sent from spammers.",
        value_parser = |s: &str| s.parse::<Address>().map_err(|_| "Invalid address format".to_string())
    )]
    pub recipient: Option<Address>,
}

#[derive(Clone, Debug)]
pub struct TransferStressArgs {
    pub amount: U256,
    pub recipient: String,
}

impl From<TransferStressCliArgs> for TransferStressArgs {
    fn from(args: TransferStressCliArgs) -> Self {
        TransferStressArgs {
            amount: args.amount,
            recipient: args
                .recipient
                .map(|addr| addr.to_string())
                .unwrap_or("{_sender}".to_owned()),
        }
    }
}

impl ToTestConfig for TransferStressArgs {
    fn to_testconfig(&self) -> contender_testfile::TestConfig {
        let TransferStressArgs { amount, recipient } = self;
        let txs = [FunctionCallDefinition::new(recipient)
            .with_value(*amount)
            .with_from_pool("spammers")]
        .into_iter()
        .map(Box::new)
        .map(SpamRequest::Tx)
        .collect::<Vec<_>>();
        contender_testfile::TestConfig {
            env: None,
            create: None,
            setup: None,
            spam: Some(txs),
        }
    }
}
