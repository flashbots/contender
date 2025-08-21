//! High-level builder/orchestrator to create a TestScenario and run a Spammer with sane defaults.
use std::{collections::HashMap, sync::Arc};

use crate::{
    agent_controller::AgentStore,
    db::DbOps,
    generator::{
        agent_pools::AgentPools,
        seeder::{rand_seed::SeedGenerator, Seeder},
        templater::Templater,
        PlanConfig,
    },
    spammer::{tx_actor::TxActorHandle, OnBatchSent, OnTxSent, Spammer},
    test_scenario::{PrometheusCollector, TestScenario, TestScenarioParams},
    Result,
};
use alloy::{consensus::TxType, signers::local::PrivateKeySigner, transports::http::reqwest::Url};
use contender_bundle_provider::bundle::BundleType;
use contender_engine_provider::AdvanceChain;

/// Unified context that captures everything needed to spin up a `TestScenario`.
pub struct ContenderCtx<D, S, P>
where
    D: DbOps + Send + Sync + 'static,
    S: Seeder + Send + Sync + Clone,
    P: PlanConfig<String> + Templater<String> + Send + Sync + Clone,
{
    pub config: P,
    pub db: Arc<D>,
    pub seeder: S,

    // Minimal required inputs:
    pub rpc_url: Url,

    // Optional extras (all defaulted):
    pub builder_rpc_url: Option<Url>,
    pub agent_store: AgentStore,
    pub user_signers: Vec<PrivateKeySigner>,
    pub tx_type: TxType,
    pub bundle_type: BundleType,
    pub pending_tx_timeout_secs: u64,
    pub extra_msg_handles: Option<HashMap<String, Arc<TxActorHandle>>>,
    pub auth_provider: Option<Arc<dyn AdvanceChain + Send + Sync + 'static>>,
    pub prometheus: PrometheusCollector,
}

impl<D, S, P> ContenderCtx<D, S, P>
where
    D: DbOps + Send + Sync + 'static,
    S: SeedGenerator + Send + Sync + Clone,
    P: PlanConfig<String> + Templater<String> + Send + Sync + Clone,
{
    pub fn builder(config: P, db: D, seeder: S, rpc_url: Url) -> ContenderCtxBuilder<D, S, P> {
        let agents = config.build_agent_store(&seeder, Default::default());
        ContenderCtxBuilder {
            config,
            db: db.into(),
            agent_store: agents,
            seeder,
            rpc_url,
            builder_rpc_url: None,
            user_signers: vec![],
            tx_type: TxType::Eip1559,
            bundle_type: BundleType::default(),
            pending_tx_timeout_secs: 12,
            extra_msg_handles: None,
            auth_provider: None,
            prometheus: PrometheusCollector::default(),
        }
    }

    /// Materialize a fresh TestScenario using the context and defaults/overrides.
    pub async fn build_scenario(&self) -> Result<TestScenario<D, S, P>> {
        let params = TestScenarioParams {
            rpc_url: self.rpc_url.clone(),
            builder_rpc_url: self.builder_rpc_url.clone(),
            signers: self.user_signers.clone(),
            agent_store: self.agent_store.clone(),
            tx_type: self.tx_type,
            pending_tx_timeout_secs: self.pending_tx_timeout_secs,
            bundle_type: self.bundle_type,
            extra_msg_handles: self.extra_msg_handles.clone(),
        };

        TestScenario::new(
            self.config.clone(),
            self.db.clone(),
            self.seeder.clone(),
            params,
            self.auth_provider.clone(),
            self.prometheus.clone(),
        )
        .await
    }
}

/// Builder with sane defaults; only (config, db, seeder, rpc_url) are required.
pub struct ContenderCtxBuilder<D, S, P>
where
    D: DbOps + Send + Sync + 'static,
    S: Seeder + Send + Sync + Clone,
    P: PlanConfig<String> + Templater<String> + Send + Sync + Clone,
{
    config: P,
    db: Arc<D>,
    seeder: S,
    rpc_url: Url,

    builder_rpc_url: Option<Url>,
    agent_store: AgentStore,
    user_signers: Vec<PrivateKeySigner>,
    tx_type: TxType,
    bundle_type: BundleType,
    pending_tx_timeout_secs: u64,
    extra_msg_handles: Option<HashMap<String, Arc<TxActorHandle>>>,
    auth_provider: Option<Arc<dyn AdvanceChain + Send + Sync + 'static>>,
    prometheus: PrometheusCollector,
}

impl<D, S, P> ContenderCtxBuilder<D, S, P>
where
    D: DbOps + Send + Sync + 'static,
    S: Seeder + Send + Sync + Clone,
    P: PlanConfig<String> + Templater<String> + Send + Sync + Clone,
{
    pub fn builder_rpc_url(mut self, url: Url) -> Self {
        self.builder_rpc_url = Some(url);
        self
    }
    pub fn agent_store(mut self, store: AgentStore) -> Self {
        self.agent_store = store;
        self
    }
    pub fn user_signers(mut self, signers: Vec<PrivateKeySigner>) -> Self {
        self.user_signers = signers;
        self
    }
    pub fn tx_type(mut self, t: TxType) -> Self {
        self.tx_type = t;
        self
    }
    pub fn bundle_type(mut self, b: BundleType) -> Self {
        self.bundle_type = b;
        self
    }
    pub fn pending_tx_timeout_secs(mut self, s: u64) -> Self {
        self.pending_tx_timeout_secs = s;
        self
    }
    pub fn extra_msg_handles(mut self, m: HashMap<String, Arc<TxActorHandle>>) -> Self {
        self.extra_msg_handles = Some(m);
        self
    }
    pub fn auth_provider(mut self, a: Arc<dyn AdvanceChain + Send + Sync + 'static>) -> Self {
        self.auth_provider = Some(a);
        self
    }
    pub fn prometheus(mut self, p: PrometheusCollector) -> Self {
        self.prometheus = p;
        self
    }

    pub fn build(self) -> ContenderCtx<D, S, P> {
        ContenderCtx {
            config: self.config,
            db: self.db,
            seeder: self.seeder,
            rpc_url: self.rpc_url,
            builder_rpc_url: self.builder_rpc_url,
            agent_store: self.agent_store,
            user_signers: self.user_signers,
            tx_type: self.tx_type,
            bundle_type: self.bundle_type,
            pending_tx_timeout_secs: self.pending_tx_timeout_secs,
            extra_msg_handles: self.extra_msg_handles,
            auth_provider: self.auth_provider,
            prometheus: self.prometheus,
        }
    }
}

/// Minimal knobs for running a spammer. Defaults are conservative.
#[derive(Clone, Copy)]
pub struct RunOpts {
    pub txs_per_period: u64,
    pub periods: u64,
    pub run_id: Option<u64>,
}

impl Default for RunOpts {
    fn default() -> Self {
        Self {
            txs_per_period: 10,
            periods: 1,
            run_id: None,
        }
    }
}

impl RunOpts {
    pub fn txs_per_period(mut self, n: u64) -> Self {
        self.txs_per_period = n;
        self
    }
    pub fn periods(mut self, n: u64) -> Self {
        self.periods = n;
        self
    }
    pub fn run_id(mut self, id: u64) -> Self {
        self.run_id = Some(id);
        self
    }
}

/// Orchestrator that plugs a built scenario into any `Spammer`.
pub struct Contender<D, S, P>
where
    D: DbOps + Send + Sync + 'static,
    S: SeedGenerator + Send + Sync + Clone,
    P: PlanConfig<String> + Templater<String> + Send + Sync + Clone,
{
    ctx: ContenderCtx<D, S, P>,
}

impl<D, S, P> Contender<D, S, P>
where
    D: DbOps + Send + Sync + 'static,
    S: SeedGenerator + Send + Sync + Clone,
    P: PlanConfig<String> + Templater<String> + Send + Sync + Clone,
{
    pub fn new(ctx: ContenderCtx<D, S, P>) -> Self {
        Self { ctx }
    }

    /// Run contract deployments and setup transactions.
    pub async fn init_scenario(&self) -> Result<()> {
        let mut scenario = self.ctx.build_scenario().await?;
        scenario.deploy_contracts().await?;
        scenario.run_setup().await?;
        Ok(())
    }

    /// Run the spammer.
    pub async fn spam<F, SP>(&self, spammer: SP, callback: Arc<F>, opts: RunOpts) -> Result<()>
    where
        F: OnTxSent + OnBatchSent + Send + Sync + 'static,
        SP: Spammer<F, D, S, P>,
    {
        let mut scenario = self.ctx.build_scenario().await?;
        spammer
            .spam_rpc(
                &mut scenario,
                opts.txs_per_period,
                opts.periods,
                opts.run_id,
                callback,
            )
            .await
    }
}
