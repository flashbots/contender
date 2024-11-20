use std::{pin::Pin, sync::Arc};

use alloy::providers::Provider;
use futures::{Stream, StreamExt};

use crate::{
    db::DbOps,
    generator::{seeder::Seeder, templater::Templater, PlanConfig},
    test_scenario::TestScenario,
};

use super::{tx_actor::TxActorHandle, OnTxSent, SpamTrigger, Spammer};

pub struct BlockwiseSpammer2<F>
where
    F: OnTxSent + Send + Sync + 'static,
{
    callback_handle: Arc<F>,
    msg_handle: Arc<TxActorHandle>,
}

impl<F> BlockwiseSpammer2<F>
where
    F: OnTxSent + Send + Sync + 'static,
{
    pub fn new<D: DbOps + Send + Sync + 'static>(
        msg_handle: TxActorHandle,
        callback_handle: F,
    ) -> Self {
        Self {
            callback_handle: Arc::new(callback_handle),
            msg_handle: Arc::new(msg_handle),
        }
    }
}

impl<F, D, S, P> Spammer<F, D, S, P> for BlockwiseSpammer2<F>
where
    F: OnTxSent + Send + Sync + 'static,
    D: DbOps + Send + Sync + 'static,
    S: Seeder + Send + Sync,
    P: PlanConfig<String> + Templater<String> + Send + Sync,
{
    fn sent_tx_callback(&self) -> std::sync::Arc<F> {
        self.callback_handle.clone()
    }

    fn msg_handler(&self) -> std::sync::Arc<TxActorHandle> {
        self.msg_handle.clone()
    }

    fn on_spam(
        &self,
        scenario: &mut TestScenario<D, S, P>,
    ) -> impl std::future::Future<Output = Pin<Box<dyn Stream<Item = SpamTrigger> + Send>>> {
        async move {
            let poller = scenario.rpc_client.watch_blocks().await.unwrap();
            let m = poller
                .into_stream()
                .flat_map(futures::stream::iter)
                .map(|b| {
                    println!("new block: {:?}", b);
                    SpamTrigger::BlockHash(b)
                })
                .boxed();
            m
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
        let msg_handle = TxActorHandle::new(12, scenario.db.clone(), scenario.rpc_client.clone());
        let spammer = BlockwiseSpammer2::new::<MockDb>(msg_handle, callback_handler);

        let result = spammer.spam_rpc(&mut scenario, 10, 3, None).await;
        println!("{:?}", result);
        assert!(result.is_ok());
    }
}
