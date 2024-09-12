use crate::Result;
use alloy::rpc::types::TransactionRequest;
pub use seeder::rand_seed::RandSeed;

pub mod seeder;
pub mod test_config;
pub mod univ2;

/// Implement Generator to programmatically
/// generate transactions for advanced testing scenarios.
pub trait Generator {
    fn get_spam_txs(&self, amount: usize) -> Result<Vec<TransactionRequest>>;
}
