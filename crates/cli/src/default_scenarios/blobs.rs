use clap::Parser;
use contender_core::generator::{types::SpamRequest, FunctionCallDefinition};
use contender_testfile::TestConfig;

use crate::default_scenarios::builtin::ToTestConfig;

#[derive(Parser, Clone, Debug)]
/// Send blob transactions. Note: the tx type will always be overridden to eip4844.
pub struct BlobsCliArgs {
    #[arg(
        short = 'd',
        long,
        long_help = "Blob data. Values can be hexidecimal or UTF-8 strings.",
        visible_aliases = ["data"],
        default_value = "0xdeadbeef"
    )]
    pub blob_data: String,

    #[arg(
        short,
        long,
        long_help = "The recipient of the blob transactions. Defaults to sender's address. May be a contract placeholder from a previous contender setup."
    )]
    pub recipient: Option<String>,
}

fn blob_txs(blob_data: impl AsRef<str>, recipient: Option<String>) -> Vec<SpamRequest> {
    vec![SpamRequest::Tx(Box::new(
        FunctionCallDefinition::new(
            recipient
                .map(|a| a.to_string())
                .unwrap_or("{_sender}".to_owned()),
        )
        .with_blob_data(blob_data),
    ))]
}

impl ToTestConfig for BlobsCliArgs {
    fn to_testconfig(&self) -> contender_testfile::TestConfig {
        TestConfig::new().with_spam(blob_txs(&self.blob_data, self.recipient.to_owned()))
    }
}
