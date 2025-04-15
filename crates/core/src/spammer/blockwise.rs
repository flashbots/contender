use std::pin::Pin;

use alloy::providers::Provider;
use futures::{Stream, StreamExt};

use crate::{
    db::DbOps,
    error::ContenderError,
    generator::{seeder::Seeder, templater::Templater, PlanConfig},
    test_scenario::TestScenario,
};

use super::{OnTxSent, SpamTrigger, Spammer};

#[derive(Default)]
pub struct BlockwiseSpammer;

impl BlockwiseSpammer {
    pub fn new() -> Self {
        Self {}
    }
}

impl<F, D, S, P> Spammer<F, D, S, P> for BlockwiseSpammer
where
    F: OnTxSent + Send + Sync + 'static,
    D: DbOps + Send + Sync + 'static,
    S: Seeder + Send + Sync + Clone,
    P: PlanConfig<String> + Templater<String> + Send + Sync + Clone,
{
    async fn on_spam(
        &self,
        scenario: &mut TestScenario<D, S, P>,
    ) -> crate::Result<Pin<Box<dyn Stream<Item = SpamTrigger> + Send>>> {
        let poller = scenario
            .rpc_client
            .watch_blocks()
            .await
            .map_err(|e| ContenderError::with_err(e, "failed to get block stream"))?;
        Ok(poller
            .into_stream()
            .flat_map(futures::stream::iter)
            .map(|b| {
                println!("new block detected: {:?}", b);
                SpamTrigger::BlockHash(b)
            })
            .boxed())
    }
}

#[cfg(test)]
mod tests {
    use alloy::{
        consensus::constants::ETH_TO_WEI,
        network::AnyNetwork,
        primitives::U256,
        providers::{DynProvider, ProviderBuilder},
    };

    use crate::{
        agent_controller::{AgentStore, SignerStore},
        db::MockDb,
        generator::util::test::spawn_anvil,
        spammer::util::test::{fund_account, get_test_signers, MockCallback},
        test_scenario::{tests::MockConfig, TestScenarioParams},
    };
    use std::collections::HashSet;
    use std::sync::Arc;

    use super::*;

    #[tokio::test]
    async fn watches_blocks_and_spams_them() {
        let anvil = spawn_anvil();
        let provider = DynProvider::new(
            ProviderBuilder::new()
                .network::<AnyNetwork>()
                .on_http(anvil.endpoint_url().to_owned()),
        );
        println!("anvil url: {}", anvil.endpoint_url());
        let seed = crate::generator::RandSeed::seed_from_str("444444444444");
        let mut agents = AgentStore::new();
        let txs_per_period = 10u64;
        let periods = 3u64;
        let tx_type = alloy::consensus::TxType::Legacy;
        let num_signers = (txs_per_period / periods) as usize;
        agents.add_agent(
            "pool1",
            SignerStore::new_random(num_signers, &seed, "eeeeeeee"),
        );
        agents.add_agent(
            "pool2",
            SignerStore::new_random(num_signers, &seed, "11111111"),
        );

        let user_signers = get_test_signers();
        let mut nonce = provider
            .get_transaction_count(user_signers[0].address())
            .await
            .unwrap();

        for (_pool_name, agent) in agents.all_agents() {
            for signer in &agent.signers {
                let res = fund_account(
                    &user_signers[0],
                    signer.address(),
                    U256::from(ETH_TO_WEI),
                    &provider,
                    Some(nonce),
                    tx_type,
                )
                .await
                .unwrap();
                println!("funded signer: {:?}", res);
                provider.watch_pending_transaction(res).await.unwrap();
                nonce += 1;
            }
        }

        let mut scenario = TestScenario::new(
            MockConfig,
            MockDb.into(),
            seed,
            TestScenarioParams {
                rpc_url: anvil.endpoint_url(),
                builder_rpc_url: None,
                signers: user_signers,
                agent_store: agents,
                tx_type,
                gas_price_percent_add: None,
                pending_tx_timeout_secs: 12,
            },
        )
        .await
        .unwrap();
        let callback_handler = MockCallback;
        let spammer = BlockwiseSpammer {};

        let start_block = provider.get_block_number().await.unwrap();

        let result = spammer
            .spam_rpc(
                &mut scenario,
                txs_per_period,
                periods,
                None,
                Arc::new(callback_handler),
            )
            .await;
        assert!(result.is_ok());

        let mut unique_addresses = HashSet::new();
        let mut n_block = start_block;
        let current_block = provider.get_block_number().await.unwrap();

        while n_block <= current_block {
            let receipts = provider.get_block_receipts(n_block.into()).await.unwrap();
            if let Some(receipts) = receipts {
                for tx in receipts {
                    unique_addresses.insert(tx.from);
                }
            }
            n_block += 1;
        }

        for addr in unique_addresses.iter() {
            println!("unique address: {}", addr);
        }

        assert!(unique_addresses.len() >= (txs_per_period / periods) as usize);
        assert!(unique_addresses.len() <= txs_per_period as usize);
    }
}
