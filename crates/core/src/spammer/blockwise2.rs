use std::{pin::Pin, sync::Arc};

use alloy::providers::Provider;
use futures::{Stream, StreamExt};

use crate::{
    db::DbOps,
    generator::{seeder::Seeder, templater::Templater, PlanConfig},
    test_scenario::{SpamTrigger, TestScenario},
};

use super::{tx_actor::TxActorHandle, OnTxSent, Spammer};

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
                    println!("[[bw2]] block: {:?}", b);
                    SpamTrigger::BlockHash(b)
                })
                .boxed();
            m
        }
    }
}
