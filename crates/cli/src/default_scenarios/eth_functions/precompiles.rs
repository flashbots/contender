use crate::default_scenarios::contracts::SPAM_ME;
use clap::ValueEnum;
use contender_core::generator::types::{FunctionCallDefinition, SpamRequest};
use strum::EnumIter;

#[derive(ValueEnum, Clone, Debug, EnumIter, PartialEq, Eq)]
// TODO: add missing precompiles to SpamMe contract & here.
pub enum EthereumPrecompile {
    #[clap(aliases = ["sha256"])]
    HashSha256,
    #[clap(aliases = ["ripemd160"])]
    HashRipemd160,
    Identity,
    #[clap(name = "modExp", aliases = ["modexp"])]
    ModExp,
    #[clap(name = "ecAdd", aliases = ["ecadd"])]
    EcAdd,
    #[clap(name = "ecMul", aliases = ["ecmul"])]
    EcMul,
    #[clap(name = "ecPairing", aliases = ["ecpairing"])]
    EcPairing,
    Blake2f,
}

impl EthereumPrecompile {
    /// Returns the function name required to call the precompile via our contract.
    fn method(&self) -> &'static str {
        match self {
            EthereumPrecompile::HashSha256 => "hash_sha256",
            EthereumPrecompile::HashRipemd160 => "hash_ripemd160",
            EthereumPrecompile::Identity => "identity",
            EthereumPrecompile::ModExp => "modExp",
            EthereumPrecompile::EcAdd => "ecAdd",
            EthereumPrecompile::EcMul => "ecMul",
            EthereumPrecompile::EcPairing => "ecPairing",
            EthereumPrecompile::Blake2f => "blake2f",
        }
    }
}

pub fn precompile_txs(args: &[EthereumPrecompile], num_iterations: u64) -> Vec<SpamRequest> {
    args.iter()
        .map(|precompile| {
            SpamRequest::Tx(FunctionCallDefinition {
                to: SPAM_ME.template_name(),
                signature: Some(
                    "callPrecompile(string memory method, uint256 iterations)".to_string(),
                ),
                args: vec![precompile.method().to_owned(), num_iterations.to_string()].into(),
                value: None,
                from: None,
                from_pool: Some("spammers".to_owned()),
                fuzz: None,
                kind: Some("precompiles".to_owned()),
                gas_limit: None,
            })
        })
        .collect()
}
