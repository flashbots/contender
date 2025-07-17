use clap::{arg, Parser};
use contender_core::generator::{types::SpamRequest, CreateDefinition, FunctionCallDefinition};
use contender_testfile::TestConfig;
use strum::IntoEnumIterator;

use crate::default_scenarios::{
    builtin::ToTestConfig,
    contracts,
    eth_functions::{
        opcodes::EthereumOpcode, precompiles::EthereumPrecompile, EthFunctionsArgs,
        EthFunctionsCliArgs,
    },
    storage::{StorageStressArgs, StorageStressCliArgs},
    transfers::{TransferStressArgs, TransferStressCliArgs},
};

#[derive(Debug, Clone, Parser)]
pub struct StressCliArgs {
    #[arg(
        long,
        long_help = "Remove storage stress txs from the scenario.",
        default_value_t = false,
        visible_aliases = &["ds"]
    )]
    pub disable_storage: bool,

    #[arg(
        long,
        long_help = "Remove transfer stress txs from the scenario.",
        default_value_t = false,
        visible_aliases = &["dt"]
    )]
    pub disable_transfers: bool,

    #[arg(
        long,
        long_help = "Comma-separated list of opcodes to be ignored in the scenario.",
        value_delimiter = ',',
        value_name = "OPCODES",
        visible_aliases = &["do"]
    )]
    pub disable_opcodes: Option<Vec<EthereumOpcode>>,

    #[arg(
        long,
        long_help = "Comma-separated list of precompiles to be ignored in the scenario.",
        value_delimiter = ',',
        value_name = "PRECOMPILES",
        visible_aliases = &["dp"]
    )]
    pub disable_precompiles: Option<Vec<EthereumPrecompile>>,

    #[arg(
        long,
        long_help = "Disable all precompiles in the scenario.",
        default_value_t = false,
        visible_aliases = &["dap"]
    )]
    pub disable_all_precompiles: bool,

    #[arg(
        long,
        long_help = "Disable all opcodes in the scenario.",
        default_value_t = false,
        visible_aliases = &["dao"]
    )]
    pub disable_all_opcodes: bool,

    #[command(flatten)]
    pub storage: StorageStressCliArgs,

    #[command(flatten)]
    pub transfers: TransferStressCliArgs,

    #[arg(
        long = "opcode-iterations",
        long_help = "Number of times to call an opcode in a single tx.",
        default_value_t = 10,
        visible_aliases = &["ops"]
    )]
    pub opcode_iterations: u64,
}

impl ToTestConfig for StressCliArgs {
    fn to_testconfig(&self) -> TestConfig {
        let mut configs = vec![];

        if !self.disable_storage {
            let args: StorageStressArgs = self.storage.to_owned().into();
            configs.push(args.to_testconfig());
        }

        if !self.disable_transfers {
            let args: TransferStressArgs = self.transfers.to_owned().into();
            configs.push(args.to_testconfig());
        }

        let opcodes = EthereumOpcode::iter()
            .filter(|opcode| {
                !self
                    .disable_opcodes
                    .as_deref()
                    .unwrap_or(&[])
                    .contains(opcode)
            })
            .filter(|opcode| {
                let o = opcode.to_owned();
                o != EthereumOpcode::Revert
                    && o != EthereumOpcode::Create2
                    && o != EthereumOpcode::Invalid
            })
            .collect::<Vec<_>>();

        let mut eth_functions: EthFunctionsArgs = EthFunctionsCliArgs {
            opcodes,
            precompiles: EthereumPrecompile::iter()
                .filter(|p| {
                    !self
                        .disable_precompiles
                        .as_deref()
                        .unwrap_or(&[])
                        .contains(p)
                })
                .collect(),
            num_iterations: self.opcode_iterations,
        }
        .into();

        if self.disable_all_precompiles {
            eth_functions.precompiles = vec![];
        }
        if self.disable_all_opcodes {
            eth_functions.opcodes = vec![];
        }

        let mut config = eth_functions.to_testconfig();
        let disabled_opcodes = self.disable_opcodes.as_deref().unwrap_or_default();
        let mut push_spam = |req: FunctionCallDefinition| {
            if let Some(spam) = config.spam.as_mut() {
                spam.push(SpamRequest::Tx(req.into()));
            }
        };

        let consume_gas = |args: &[&'static str]| {
            FunctionCallDefinition::new(contracts::SPAM_ME.template_name())
                .with_signature("consumeGas(string memory method, uint256 iterations)")
                .with_args(args)
                .with_from_pool("spammers")
        };

        // custom overrides for specific opcodes
        if !self.disable_all_opcodes {
            if !disabled_opcodes.contains(&EthereumOpcode::Revert) {
                let tx = consume_gas(&["revert", "1"]).with_gas_limit(42000);
                push_spam(tx);
            }
            if !disabled_opcodes.contains(&EthereumOpcode::Create2) {
                let tx = consume_gas(&["create2", "1"]);
                push_spam(tx);
            }
            if !disabled_opcodes.contains(&EthereumOpcode::Invalid) {
                let tx = consume_gas(&["invalid", "1"]).with_gas_limit(21000);
                push_spam(tx);
            }
        }
        configs.push(config);

        // compile all configs into a single TestConfig
        let txs = configs
            .into_iter()
            .flat_map(|config| config.spam.unwrap_or_default())
            .collect::<Vec<_>>();

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
