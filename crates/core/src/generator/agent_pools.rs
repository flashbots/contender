use crate::{
    agent_controller::{AgentClass, AgentStore},
    generator::{seeder::rand_seed::SeedGenerator, PlanConfig},
};

pub trait AgentPools {
    fn build_agent_store(&self, seed: &impl SeedGenerator, agent_spec: AgentSpec) -> AgentStore;
}

/// Defines the number of accounts to generate for each category of agent: creators, setter-uppers, and spammers.
/// "Agents" in contender are referred to by name (defined by `from_pool` in scenario specs) and may hold many accounts.
#[derive(Clone, Debug)]
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

impl AgentSpec {
    pub fn create_accounts(mut self, count: usize) -> Self {
        self.create_accounts = count;
        self
    }

    pub fn setup_accounts(mut self, count: usize) -> Self {
        self.setup_accounts = count;
        self
    }

    pub fn spam_accounts(mut self, count: usize) -> Self {
        self.spam_accounts = count;
        self
    }
}

impl<P> AgentPools for P
where
    P: PlanConfig<String>,
{
    fn build_agent_store(&self, seed: &impl SeedGenerator, agent_spec: AgentSpec) -> AgentStore {
        use std::collections::HashMap;

        // Collect pools with their required signer counts
        let pools_with_counts = [
            (
                self.get_create_pools(),
                agent_spec.create_accounts,
                AgentClass::Deployer,
            ),
            (
                self.get_setup_pools(),
                agent_spec.setup_accounts,
                AgentClass::SetupSender,
            ),
            (
                self.get_spam_pools(),
                agent_spec.spam_accounts,
                AgentClass::Spammer,
            ),
        ];

        // Build a map of pool_name -> max signers needed across all categories.
        // This ensures pools used in multiple categories (e.g., "admin" in both create and spam)
        // get the maximum number of signers needed.
        let mut pool_max_signers: HashMap<String, (usize, AgentClass)> = HashMap::new();
        for (pools, count, agent_class) in pools_with_counts {
            for pool in pools {
                pool_max_signers
                    .entry(pool)
                    .and_modify(|(c, _)| *c = (*c).max(count))
                    .or_insert((count, agent_class.clone()));
            }
        }

        let mut agents = AgentStore::new();
        for (pool_name, (max_signers, agent_class)) in pool_max_signers {
            if max_signers > 0 {
                agents.add_new_agent(&pool_name, max_signers, seed, agent_class);
            }
        }

        agents
    }
}
