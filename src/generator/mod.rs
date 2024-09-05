use crate::Result;
use alloy::rpc::types::TransactionRequest;
use rand_seed::RandSeed;

pub mod rand_seed;
pub mod test_config;
pub mod univ2;

/// Implement Generator to programmatically
/// generate transactions for advanced testing scenarios.
pub trait Generator {
    fn get_spam_txs(
        &self,
        amount: usize,
        seed: Option<RandSeed>,
    ) -> Result<Vec<TransactionRequest>>;
}
