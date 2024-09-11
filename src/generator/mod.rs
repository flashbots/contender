use crate::Result;
use alloy::rpc::types::TransactionRequest;
pub use rand_seed::RandSeed;
use rand_seed::Seeder;

pub mod rand_seed;
pub mod test_config;
pub mod univ2;

/// Implement Generator to programmatically
/// generate transactions for advanced testing scenarios.
pub trait Generator<T: Seeder> {
    fn get_spam_txs(&self, amount: usize, seed: &T) -> Result<Vec<TransactionRequest>>;
}
