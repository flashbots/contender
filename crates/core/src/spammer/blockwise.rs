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
    S: Seeder + Send + Sync,
    P: PlanConfig<String> + Templater<String> + Send + Sync,
{
    fn on_spam(
        &self,
        scenario: &mut TestScenario<D, S, P>,
    ) -> impl std::future::Future<Output = crate::Result<Pin<Box<dyn Stream<Item = SpamTrigger> + Send>>>>
    {
        async move {
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
}

#[cfg(test)]
mod tests {
    use crate::{
        agent_controller::AgentStore,
        db::MockDb,
        generator::util::test::spawn_anvil,
        spammer::util::test::{get_test_signers, MockCallback},
        test_scenario::tests::MockConfig,
    };
    use std::sync::Arc;

    use super::*;

    #[tokio::test]
    async fn watches_blocks_and_spams_them() {
        let anvil = spawn_anvil();
        println!("anvil url: {}", anvil.endpoint_url());
        let seed = crate::generator::RandSeed::from_str("444444444444");
        let mut scenario = TestScenario::new(
            MockConfig,
            MockDb.into(),
            anvil.endpoint_url(),
            None,
            seed,
            get_test_signers().as_slice(),
            AgentStore::new(),
        )
        .await
        .unwrap();
        let callback_handler = MockCallback;
        let spammer = BlockwiseSpammer {};

        let result = spammer
            .spam_rpc(&mut scenario, 10, 3, None, Arc::new(callback_handler))
            .await;
        println!("{:?}", result);
        assert!(result.is_ok());
    }
}
