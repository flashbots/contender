use crate::default_scenarios::contracts::SPAM_ME;
use clap::ValueEnum;
use contender_core::generator::{types::SpamRequest, FunctionCallDefinition};
use strum::EnumIter;

#[derive(ValueEnum, Clone, Debug, strum::Display, EnumIter, PartialEq, Eq)]
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
            SpamRequest::Tx(
                FunctionCallDefinition::new(SPAM_ME.template_name())
                    .with_signature("callPrecompile(string memory method, uint256 iterations)")
                    .with_args(&[precompile.method().to_owned(), num_iterations.to_string()])
                    .with_from_pool("spammers")
                    .with_kind("precompiles"),
            )
        })
        .collect()
}
