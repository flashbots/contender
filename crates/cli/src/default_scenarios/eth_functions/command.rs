use crate::default_scenarios::{
    builtin::ToTestConfig,
    contracts,
    eth_functions::{
        opcodes::{opcode_txs, EthereumOpcode},
        precompiles::{precompile_txs, EthereumPrecompile},
    },
};
use clap::Parser;
use contender_core::generator::CreateDefinition;
use contender_testfile::TestConfig;

#[derive(Parser, Clone, Debug)]
pub struct EthFunctionsCliArgs {
    #[arg(
        short,
        long,
        value_delimiter = ',',
        value_name = "OPCODES",
        long_help = "Comma-separated list of opcodes to call in spam transactions."
    )]
    pub opcodes: Vec<EthereumOpcode>,

    #[arg(
        short,
        long,
        value_delimiter = ',',
        value_name = "PRECOMPILES",
        long_help = "Comma-separated list of precompiles to call in spam transactions."
    )]
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
                signature: None,
                args: None,
                from: None,
                from_pool: Some("admin".to_owned()),
            }]),
            setup: None,
            spam: Some(txs),
        }
    }
}
