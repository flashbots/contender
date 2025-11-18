use crate::{
    commands::common::parse_amount, default_scenarios::setcode::SetCodeCliArgs,
    error::ContenderError,
};
use alloy::{hex::ToHexExt, primitives::U256};
use clap::Parser;
use contender_core::generator::{error::GeneratorError, util::encode_calldata};

pub const DEFAULT_SIG: &str = "execute((address,uint256,bytes)[])";
pub const DEFAULT_ARGS: &str = "[(0x{Counter},0,0xd09de08a)]";

#[derive(Clone, Debug, Parser)]
pub struct SetCodeExecuteCliArgs {
    /// The address to call via the smart-wallet's execute function.
    #[arg(
        long,
        value_name = "ADDRESS",
        help = "Address to call via the smart-wallet's execute function."
    )]
    pub to: String,

    /// The solidity signature of the function to call via the smart-wallet's execute function.
    #[arg(
        long = "sig",
        value_name = "SIGNATURE",
        long_help = "Signature of the function to call via the smart-wallet's execute function.
Example:
--sig \"setNumber(uint256 num)\""
    )]
    pub sig: String,

    /// Comma-separated arguments to the function being called via the smart-wallet's execute function.
    #[arg(
        long,
        value_name = "ARGS",
        long_help = "Comma-separated arguments to the function being called via the smart-wallet's execute function.
Examples:
--args 42
--args \"9001,0xf00d\"",
        value_delimiter = ','
    )]
    pub args: Vec<String>,

    /// Ether to send with the delegated call. Note: you must manually fund your setCode signer's account to use this feature.
    #[arg(
        long,
        value_name = "VALUE_WITH_UNITS",
        long_help = "Ether to send with the delegated call.
Note: you must manually fund your setCode signer's account to use this feature. Use `contender admin setcode-signer` to get this account's details.
Example:
--value \"0.01 eth\"",
        value_parser = parse_amount
    )]
    pub value: Option<U256>,
}

#[derive(clap::Subcommand, Clone, Debug)]
pub enum SetCodeSubCommand {
    /// Helper function to delegate function calls via `execute(Call[])` on a smart-wallet contract.
    Execute(SetCodeExecuteCliArgs),
}

impl SetCodeExecuteCliArgs {
    pub fn to_setcode_cli_args(
        &self,
        og_args: &SetCodeCliArgs,
    ) -> Result<SetCodeCliArgs, ContenderError> {
        Ok(SetCodeCliArgs {
            contract_address: og_args.contract_address.to_owned(),
            command: og_args.command.to_owned(),
            signature: Some(DEFAULT_SIG.to_string()),
            args: Some(vec![format!(
                "[(0x{},{},{})]",
                self.to.trim_start_matches("0x"),
                self.value.unwrap_or(U256::ZERO).to_string(),
                encode_calldata(&self.args, &self.sig)
                    .map_err(|e| contender_core::Error::Generator(GeneratorError::Util(e)))?
                    .encode_hex(),
            )]),
        })
    }
}
