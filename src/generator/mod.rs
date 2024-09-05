pub mod rand_seed;
pub mod test_config;
pub mod univ2;

use crate::Result;
use alloy::rpc::types::TransactionRequest;
use rand_seed::RandSeed;

/// Implement SpamTarget for a specific contract to programmatically
/// generate templates for advanced testing scenarios.
pub trait SpamTarget {
    fn get_spam_txs(
        &self,
        amount: usize,
        seed: Option<RandSeed>,
    ) -> Result<Vec<TransactionRequest>>;
}
