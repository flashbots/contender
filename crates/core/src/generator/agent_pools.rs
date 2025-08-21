use crate::{
    agent_controller::AgentStore,
    generator::{seeder::rand_seed::SeedGenerator, PlanConfig},
};

pub trait AgentPools {
    fn build_agent_store(&self, seed: &impl SeedGenerator, agent_spec: AgentSpec) -> AgentStore;
}

/// Defines the number of accounts to generate for each category of agent: creators, setter-uppers, and spammers.
/// "Agents" in contender are referred to by name (defined by `from_pool` in scenario specs) and may hold many accounts.
#[derive(Debug)]
pub struct AgentSpec {
    /// number of accounts to generate per `create` agent
    create_accounts: usize,

    /// number of accounts to generate per `setup` agent
    setup_accounts: usize,

    /// number of accounts to generate per `spam` agent
    spam_accounts: usize,
}

impl Default for AgentSpec {
    fn default() -> Self {
        AgentSpec {
            create_accounts: 1,
            setup_accounts: 1,
            spam_accounts: 10,
        }
    }
}

impl<P> AgentPools for P
where
    P: PlanConfig<String>,
{
    fn build_agent_store(&self, seed: &impl SeedGenerator, agent_spec: AgentSpec) -> AgentStore {
        let create_pools = self.get_create_pools();
        let setup_pools = self.get_setup_pools();
        let spam_pools = self.get_spam_pools();

        let mut agents = AgentStore::new();
        agents.init(&create_pools, agent_spec.create_accounts, seed);
        agents.init(&setup_pools, agent_spec.setup_accounts, seed);
        agents.init(&spam_pools, agent_spec.spam_accounts, seed);

        agents
    }
}
