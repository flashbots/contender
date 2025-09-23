//! High-level builder/orchestrator to create a TestScenario and run a Spammer with sane defaults.
//! Generally simplifies instantiation for library users, while maintaining flexibility by
//! providing methods to override defaults.
use std::{collections::HashMap, ops::Deref, str::FromStr, sync::Arc, time::Duration};

use crate::{
    agent_controller::AgentStore,
    db::{DbOps, MockDb, SpamDuration, SpamRunRequest},
    error::ContenderError,
    generator::{
        agent_pools::AgentPools,
        seeder::{rand_seed::SeedGenerator, Seeder},
        templater::Templater,
        PlanConfig, RandSeed,
    },
    spammer::{tx_actor::TxActorHandle, OnBatchSent, OnTxSent, Spammer},
    test_scenario::{PrometheusCollector, TestScenario, TestScenarioParams},
    util::default_signers,
    Result,
};
use alloy::{
    consensus::TxType, node_bindings::WEI_IN_ETHER, primitives::U256,
    signers::local::PrivateKeySigner, transports::http::reqwest::Url,
};
use contender_bundle_provider::bundle::BundleType;
use contender_engine_provider::ControlChain;
use std::sync::LazyLock;

static SMOL_AMOUNT: LazyLock<U256> = LazyLock::new(|| WEI_IN_ETHER / U256::from(100));

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
    pub auth_provider: Option<Arc<dyn ControlChain + Send + Sync + 'static>>,
    pub prometheus: PrometheusCollector,
    /// The amount of ether each agent account gets.
    pub funding: U256,
    /// Redeploys contracts that have already been deployed.
    pub redeploy: bool,
}

impl<P> ContenderCtx<MockDb, RandSeed, P>
where
    P: PlanConfig<String> + Templater<String> + Send + Sync + Clone,
{
    /// Constructs a [`ContenderCtxBuilder`] with a mock db and random seed.
    /// Convenient for testing simple scenarios where no setup is required.
    ///
    /// **Note:** Scenarios with `create` or `setup` steps are not supported.
    /// For those configs, use [`ContenderCtx::builder`] instead.
    ///
    /// ## Panics
    ///
    /// * When scenario configs with `create` or `setup` steps are passed.
    ///
    /// * When an invalid `rpc_url` is passed.
    ///
    /// ## Example:
    ///
    /// ```rs
    /// use contender_core::ContenderCtx;
    /// use contender_testfile::TestConfig;
    ///
    /// let config = TestConfig::new(); // .with_spam_steps(...)
    /// let ctx = ContenderCtx::builder_simple(config, "http://localhost:8545").build();
    /// ```
    pub fn builder_simple(
        config: P,
        rpc_url: impl AsRef<str>,
    ) -> ContenderCtxBuilder<MockDb, RandSeed, P> {
        if [
            config.get_create_steps().unwrap_or_default().len(),
            config.get_setup_steps().unwrap_or_default().len(),
        ]
        .iter()
        .any(|len| *len > 0)
        {
            // a real DB is required to run these steps -- panic
            tracing::error!("This builder does not support scenario configs with create/setup steps. Try ContenderCtx::builder_simple instead.");
            panic!("create/setup steps not supported by this builder.");
        }

        let seed = RandSeed::new();
        let db = MockDb;
        let agents = config.build_agent_store(&seed, Default::default());
        let rpc_url = Url::from_str(rpc_url.as_ref()).expect("invalid RPC URL");
        ContenderCtxBuilder {
            config,
            db: db.into(),
            seeder: seed,
            rpc_url,
            builder_rpc_url: None,
            agent_store: agents,
            user_signers: default_signers(),
            tx_type: TxType::Eip1559,
            bundle_type: BundleType::default(),
            pending_tx_timeout_secs: 12,
            extra_msg_handles: None,
            auth_provider: None,
            prometheus: PrometheusCollector::default(),
            funding: *SMOL_AMOUNT,
            redeploy: false,
        }
    }
}

impl<D, S, P> ContenderCtx<D, S, P>
where
    D: DbOps + Send + Sync + 'static,
    S: SeedGenerator + Send + Sync + Clone,
    P: PlanConfig<String> + Templater<String> + Send + Sync + Clone,
{
    /// Constructs a [`ContenderCtxBuilder`] with a user-provided DB & seeder.
    ///
    /// ## Params
    ///
    /// ### `config`
    ///
    /// This contains all `create`, `setup`, and `spam` steps, as well as `env` values.
    /// [`contender_testfile::TestConfig`] is recommended
    ///
    /// ### `db`
    ///
    /// Contender stores contract addresses and transaction results in the DB passed here.
    /// [`contender_sqlite::SqliteDb`] is recommended.
    ///
    /// ### `seeder`
    ///
    /// Used to repeatably generate pseudo-random numbers for account creation & fuzz params.
    /// [`contender_core::generator::RandSeed`] is recommended.
    ///
    /// ### `rpc_url`
    ///
    /// Where the transactions are sent.
    ///
    /// ## Panics
    ///
    /// - When an invalid `rpc_url` is provided
    ///
    /// ## Example
    ///
    /// ```rs
    /// use contender_sqlite::SqliteDb;
    /// use contender_core::{ContenderCtx, generator::RandSeed};
    /// use contender_testfile::TestConfig;
    ///
    /// let config = TestConfig::new(); // .with_spam_steps(...)
    /// let db = SqliteDb::new_memory();
    /// let seeder = RandSeed::new();
    /// let ctx = ContenderCtx::builder(config, db, seeder, "http://localhost:8545").build();
    /// ```
    pub fn builder(
        config: P,
        db: D,
        seeder: S,
        rpc_url: impl AsRef<str>,
    ) -> ContenderCtxBuilder<D, S, P> {
        let agents = config.build_agent_store(&seeder, Default::default());
        let url = Url::from_str(rpc_url.as_ref()).expect("invalid RPC URL");
        ContenderCtxBuilder {
            config,
            db: db.into(),
            agent_store: agents,
            seeder,
            rpc_url: url,
            builder_rpc_url: None,
            user_signers: default_signers(),
            tx_type: TxType::Eip1559,
            bundle_type: BundleType::default(),
            pending_tx_timeout_secs: 12,
            extra_msg_handles: None,
            auth_provider: None,
            prometheus: PrometheusCollector::default(),
            funding: *SMOL_AMOUNT,
            redeploy: false,
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
            redeploy: self.redeploy,
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
    auth_provider: Option<Arc<dyn ControlChain + Send + Sync + 'static>>,
    prometheus: PrometheusCollector,
    funding: U256,
    redeploy: bool,
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
    pub fn auth_provider(mut self, a: Arc<dyn ControlChain + Send + Sync + 'static>) -> Self {
        self.auth_provider = Some(a);
        self
    }
    pub fn prometheus(mut self, p: PrometheusCollector) -> Self {
        self.prometheus = p;
        self
    }
    pub fn funding(mut self, f: U256) -> Self {
        self.funding = f;
        self
    }
    pub fn seeder(mut self, s: S) -> Self {
        self.seeder = s;
        self
    }
    pub fn redeploy(mut self, r: bool) -> Self {
        self.redeploy = r;
        self
    }

    pub fn build(self) -> ContenderCtx<D, S, P> {
        // always try to create tables before building, so user doesn't have to think about it later.
        // we'll get an error if the tables already exist, but it's safe to ignore
        self.db.create_tables().unwrap_or_default();

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
            funding: self.funding,
            redeploy: self.redeploy,
        }
    }
}

/// Minimal knobs for running a spammer. Defaults are conservative.
#[derive(Clone)]
pub struct RunOpts {
    pub txs_per_period: u64,
    pub periods: u64,
    pub name: String,
}

impl Default for RunOpts {
    fn default() -> Self {
        Self {
            txs_per_period: 10,
            periods: 1,
            name: "Unknown".to_owned(),
        }
    }
}

impl RunOpts {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn txs_per_period(mut self, n: u64) -> Self {
        self.txs_per_period = n;
        self
    }
    pub fn periods(mut self, n: u64) -> Self {
        self.periods = n;
        self
    }
    pub fn name(mut self, name: impl AsRef<str>) -> Self {
        self.name = name.as_ref().to_owned();
        self
    }

    pub fn create_spam_run_request(
        &self,
        rpc_url: impl AsRef<str>,
        pending_timeout: Duration,
        spam_duration: SpamDuration,
    ) -> SpamRunRequest {
        SpamRunRequest {
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("Time went backwards")
                .as_millis() as usize,
            tx_count: (self.periods * self.txs_per_period) as usize,
            scenario_name: self.name.to_owned(),
            rpc_url: rpc_url.as_ref().to_owned(),
            txs_per_duration: self.txs_per_period,
            duration: spam_duration,
            pending_timeout,
        }
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
    initialized: bool,
}

impl<D, S, P> Contender<D, S, P>
where
    D: DbOps + Send + Sync + 'static,
    S: SeedGenerator + Send + Sync + Clone,
    P: PlanConfig<String> + Templater<String> + Send + Sync + Clone,
{
    /// Create a Contender instance.
    ///
    /// Example:
    /// ```
    /// use std::sync::Arc;
    /// use contender_core::{ContenderCtx, Contender, RunOpts, spammer::{NilCallback, TimedSpammer}};
    /// use contender_testfile::TestConfig;
    ///
    /// // Create a config for your test scenario
    /// let config = TestConfig::new(); // configure as needed
    ///
    /// // Initialize the context with builder_simple
    /// let ctx = ContenderCtx::builder_simple(config, "http://localhost:8545").build();
    ///
    /// // Instantiate the orchestrator & spammer
    /// let contender = Contender::new(ctx);
    /// let spammer = TimedSpammer::new(std::time::Duration::from_secs(1));
    ///
    /// // Run the spammer (async context required)
    /// // contender.spam(spammer, NilCallback.into(), RunOpts::default()).await.unwrap();
    /// ```
    pub fn new(ctx: ContenderCtx<D, S, P>) -> Self {
        Self {
            ctx,
            initialized: false,
        }
    }

    /// Funds agent accounts, then runs contract deployments and setup transactions.
    ///
    /// Run this before calling `spam`.
    pub async fn initialize(&mut self) -> Result<()> {
        let mut scenario = self.ctx.build_scenario().await?;
        for agent in scenario.agent_store.all_agents() {
            agent
                .1
                .fund_signers(
                    &self.ctx.user_signers[0],
                    self.ctx.funding,
                    scenario.rpc_client.to_owned(),
                )
                .await
                .map_err(|e| ContenderError::with_err(e.deref(), "failed to fund agent signers"))?;
        }
        scenario.deploy_contracts().await?;
        scenario.run_setup().await?;
        self.initialized = true;
        Ok(())
    }

    /// Run the spammer.
    ///
    /// ## Params
    ///
    /// ### `spammer`
    ///
    /// Defines how/when spam transactions are sent. `contender_core` includes `TimedSpammer` and `BlockwiseSpammer`.
    ///
    /// ### `callback`
    ///
    /// Defines custom behavior to be executed after transactions are sent.
    /// `contender_core` includes `NilCallback` and `LogCallback`.
    /// `NilCallback` does nothing, while `LogCallback` saves tx results to the DB.
    ///
    /// ### `opts`
    ///
    /// Defines the rate & duration of the spam run.
    ///
    /// ## Example
    ///
    /// ```rs
    /// use contender_core::spammer::{TimedSpammer, NilCallback};
    ///
    /// // Create a config for your test scenario
    /// let config = TestConfig::new(); // configure as needed
    ///
    /// // Initialize the context with builder_simple
    /// let ctx = ContenderCtx::builder_simple(config, "http://localhost:8545").build();
    ///
    /// // Instantiate the orchestrator
    /// let contender = Contender::new(ctx);
    ///
    /// // initialize timed spammer; sends a batch of txs every second
    /// let spammer = TimedSpammer::new(std::time::Duration::from_secs(1));
    /// // initialize nil callback; does nothing in response to txs being sent
    /// let callback = NilCallback;
    /// // initialize opts; slightly tweaking the defaults
    /// let opts = RunOpts::new().txs_per_period(50).periods(10);
    ///
    /// // run spammer
    /// contender.spam(spammer, callback.into(), opts).await.unwrap();
    /// ```
    pub async fn spam<F, SP>(&mut self, spammer: SP, callback: Arc<F>, opts: RunOpts) -> Result<()>
    where
        F: OnTxSent + OnBatchSent + Send + Sync + 'static,
        SP: Spammer<F, D, S, P>,
    {
        // call self.initialize if it hasn't yet been called manually
        if !self.initialized {
            self.initialize().await?;
        }

        // build scenario so we can use its DB
        let mut scenario = self.ctx.build_scenario().await?;

        // add run to DB
        let run_req = opts.create_spam_run_request(
            &scenario.rpc_url,
            Duration::from_secs(self.ctx.pending_tx_timeout_secs),
            SP::duration_units(opts.periods),
        );
        let run_id = scenario.db.insert_run(&run_req)?;

        // send spam
        spammer
            .spam_rpc(
                &mut scenario,
                opts.txs_per_period,
                opts.periods,
                Some(run_id),
                callback,
            )
            .await
    }
}
