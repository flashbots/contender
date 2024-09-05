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

/// The main RPC spam controller; sends transactions to the RPC at a given rate.
pub fn spam_rpc(
    testfile: &str,
    rpc_url: &str,
    tx_per_second: usize,
    duration: usize,
) -> Result<()> {
    // TODO: actually do this stuff:
    println!("Using testfile: {}", testfile);
    println!(
        "Spamming {} with {} tx/s for {} seconds.",
        rpc_url, tx_per_second, duration
    );

    // TODO: use MySQL or SQLite to store run data
    Ok(())
}
