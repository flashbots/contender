use crate::default_scenarios::builtin::ToTestConfig;
use contender_core::error::ContenderError;
use contender_core::generator::CompiledContract;
use contender_core::Result;
use std::process::Command;

#[derive(Clone, Debug, clap::Parser)]
pub struct CustomContractCliArgs {
    /// Path to smart contract source. Format: <path/to/contract.sol>:<ContractName>
    contract_path: std::path::PathBuf,

    #[arg(
        short = 'a',
        long,
        visible_aliases = &["args"]
    )]
    constructor_args: Vec<String>,
}

/// This contract is expected to have its constructor args already appended to the bytecode, so it's ready to deploy.
#[derive(Clone, Debug)]
pub struct CustomContractArgs {
    pub contract: CompiledContract,
}

impl CustomContractArgs {
    pub fn from_cli_args(args: CustomContractCliArgs) -> Result<Self> {
        // read smart contract src
        // format should be <path/to/contract.sol>:<ContractName>
        let contract_path = args
            .contract_path
            .to_str()
            .ok_or(ContenderError::GenericError(
                "invalid contract path",
                String::new(),
            ))?;
        
        
        
        
        // if !contract_path.ends_with(".sol") {
        //     return Err(ContenderError::GenericError("invalid contract; must be a .sol file", contract_path.to_owned()));
        // }

        // compile with forge
        let _res = Command::new("forge").args(["build", "-o", "/tmp/contender-contracts", contract_path]);

        // read artifact
        let build_artifact = std::fs::read(format!("/tmp/contender-contracts/"))

        Ok(CustomContractArgs {
            contract: CompiledContract {
                bytecode: (),
                name: (),
            },
        })
    }
}

impl ToTestConfig for CustomContractArgs {
    fn to_testconfig(&self) -> contender_testfile::TestConfig {
        todo!()
    }
}
