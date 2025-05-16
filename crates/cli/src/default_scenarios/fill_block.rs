use clap::{arg, Parser};

#[derive(Parser, Clone, Debug)]
/// Taken from the CLI, this is used to fill a block with transactions.
pub struct FillBlockCliArgs {
    #[arg(short = 'g', long, long_help = "Override gas used per block. By default, the block limit is used.", visible_aliases = ["gas"])]
    pub max_gas_per_block: Option<u64>,
}

#[derive(Clone, Debug)]
/// Full arguments for the fill-block scenario.
pub struct FillBlockArgs {
    pub max_gas_per_block: u64,
    pub num_txs: u64,
}
