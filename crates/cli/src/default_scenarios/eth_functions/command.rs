use crate::default_scenarios::{
    builtin::ToTestConfig,
    contracts,
    eth_functions::{
        opcodes::{opcode_txs, EthereumOpcode},
        precompiles::{precompile_txs, EthereumPrecompile},
    },
};
use clap::{arg, Parser};
use contender_core::generator::types::CreateDefinition;
use contender_testfile::TestConfig;

#[derive(Parser, Clone, Debug)]
/// Taken from the CLI, this is used to spam specific opcodes.
pub struct EthFunctionsCliArgs {
    #[arg(short, action = clap::ArgAction::Append, long = "opcode", long_help = "Opcodes to call in spam transactions. May be specified multiple times.")]
    pub opcodes: Vec<EthereumOpcode>,

    #[arg(short, action = clap::ArgAction::Append, long = "precompile", long_help = "Precompiles to call in spam transactions. May be specified multiple times.")]
    pub precompiles: Vec<EthereumPrecompile>,

    #[arg(
        short,
        long,
        default_value = "10",
        long_help = "Number of times to call an opcode/precompile in a single transaction."
    )]
    pub num_iterations: u64,
}

#[derive(Clone, Debug)]
pub struct EthFunctionsArgs {
    pub opcodes: Vec<EthereumOpcode>,
    pub precompiles: Vec<EthereumPrecompile>,
    pub num_iterations: u64,
}

impl From<EthFunctionsCliArgs> for EthFunctionsArgs {
    fn from(args: EthFunctionsCliArgs) -> Self {
        EthFunctionsArgs {
            opcodes: args.opcodes,
            precompiles: args.precompiles,
            num_iterations: args.num_iterations,
        }
    }
}

impl From<&EthFunctionsCliArgs> for EthFunctionsArgs {
    fn from(args: &EthFunctionsCliArgs) -> Self {
        args.to_owned().into()
    }
}

impl ToTestConfig for EthFunctionsArgs {
    fn to_testconfig(&self) -> TestConfig {
        let EthFunctionsArgs {
            opcodes,
            precompiles,
            num_iterations,
        } = self;
        let precompile_txs = precompile_txs(precompiles, *num_iterations);
        let opcode_txs = opcode_txs(opcodes, *num_iterations);
        let txs = [precompile_txs, opcode_txs].concat();

        TestConfig {
            env: None,
            create: Some(vec![CreateDefinition {
                contract: contracts::SPAM_ME.into(),
                from: None,
                from_pool: Some("admin".to_owned()),
            }]),
            setup: None,
            spam: Some(txs),
        }
    }
}
