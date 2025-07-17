use crate::default_scenarios::contracts::SPAM_ME;
use clap::ValueEnum;
use contender_core::generator::{types::SpamRequest, FunctionCallDefinition};
use strum::EnumIter;

#[derive(ValueEnum, Clone, Debug, strum::Display, EnumIter, PartialEq, Eq)]
pub enum EthereumOpcode {
    Stop,
    Add,
    Mul,
    Sub,
    Div,
    Sdiv,
    Mod,
    Smod,
    Addmod,
    Mulmod,
    Exp,
    Signextend,
    Lt,
    Gt,
    Slt,
    Sgt,
    Eq,
    Iszero,
    And,
    Or,
    Xor,
    Not,
    Byte,
    Shl,
    Shr,
    Sar,
    Sha3,
    Keccak256,
    Address,
    Balance,
    Origin,
    Caller,
    Callvalue,
    Calldataload,
    Calldatasize,
    Calldatacopy,
    Codesize,
    Codecopy,
    Gasprice,
    Extcodesize,
    Extcodecopy,
    Returndatasize,
    Returndatacopy,
    Extcodehash,
    Blockhash,
    Coinbase,
    Timestamp,
    Number,
    Prevrandao,
    Gaslimit,
    Chainid,
    Selfbalance,
    Basefee,
    Pop,
    Mload,
    Mstore,
    Mstore8,
    Sload,
    Sstore,
    Msize,
    Gas,
    Log0,
    Log1,
    Log2,
    Log3,
    Log4,
    Create,
    Call,
    Callcode,
    Return,
    Delegatecall,
    Create2,
    Staticcall,
    Revert,
    Invalid,
    Selfdestruct,
}

pub fn opcode_txs(args: &[EthereumOpcode], num_iterations: u64) -> Vec<SpamRequest> {
    args.iter()
        .map(|opcode| {
            SpamRequest::Tx(
                FunctionCallDefinition::new(SPAM_ME.template_name())
                    .with_signature("consumeGas(string memory method, uint256 iterations)")
                    .with_args(&[
                        format!("{opcode:?}").to_lowercase(),
                        num_iterations.to_string(),
                    ])
                    .with_from_pool("spammers")
                    .with_kind("opcodes"),
            )
        })
        .collect()
}
