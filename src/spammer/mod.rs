use crate::{
    generator::{rand_seed::RandSeed, test_config::TestConfig, SpamTarget},
    Result,
};
use tokio::task::spawn as spawn_task;

pub struct Spammer {
    testfile: TestConfig,
    rpc_url: String,
    seed: RandSeed,
    // TODO: add wallet/client to send txs
}

impl Spammer {
    pub fn new(testfile: TestConfig, rpc_url: String, seed: Option<RandSeed>) -> Self {
        let seed = seed.unwrap_or_default();
        Self {
            testfile,
            rpc_url,
            seed,
        }
    }

    /// Send transactions to the RPC at a given rate. Actual rate may vary; this is only the attempted sending rate.
    pub fn spam_rpc(&self, tx_per_second: usize, duration: usize) -> Result<()> {
        let tx_requests = self
            .testfile
            .get_spam_txs(tx_per_second * duration, Some(self.seed.to_owned()))?;
        let interval = std::time::Duration::from_millis(1_000 / tx_per_second as u64);

        for tx in tx_requests {
            // send tx to the RPC asynchrononsly
            spawn_task(async move {
                // TODO: sign tx and send to RPC here.
                println!("sending tx: {:?}", tx);
                drop(tx);
            });

            // sleep for interval
            std::thread::sleep(interval);
        }

        Ok(())
    }
}
