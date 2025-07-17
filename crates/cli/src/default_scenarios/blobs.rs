use alloy::primitives::Address;
use clap::{arg, Parser};
use contender_core::generator::{types::SpamRequest, FunctionCallDefinition};
use contender_testfile::TestConfig;

use crate::default_scenarios::builtin::ToTestConfig;

#[derive(Parser, Clone, Debug)]
/// Send blob transactions. Note: the tx type will always be overridden to eip4844.
pub struct BlobsCliArgs {
    #[arg(
        short = 'd',
        long,
        long_help = "Blob data. Values are assumed to be hexadecimal.",
        visible_aliases = ["data"],
        default_value = "0xdeadbeef"
    )]
    pub blob_data: String,

    #[arg(
        short,
        long,
        long_help = "The recipient of the blob transactions. Defaults to sender's address.",
        visible_aliases = &["address"]
    )]
    pub recipient: Option<Address>,
}

fn blob_txs(blob_data: impl AsRef<str>, recipient: Option<Address>) -> Vec<SpamRequest> {
    vec![SpamRequest::Tx(Box::new(
        FunctionCallDefinition::new(
            recipient
                .map(|a| a.to_string())
                .unwrap_or("{_sender}".to_owned()),
        )
        .with_from_pool("spammers")
        .with_blob_data(blob_data),
    ))]
}

impl ToTestConfig for BlobsCliArgs {
    fn to_testconfig(&self) -> contender_testfile::TestConfig {
        TestConfig {
            env: None,
            create: None,
            setup: None,
            spam: Some(blob_txs(self.blob_data.to_owned(), self.recipient)),
        }
    }
}
