use std::{collections::HashMap, sync::Arc};

use alloy::{
    network::{AnyNetwork, AnyTxEnvelope, EthereumWallet, TransactionBuilder},
    primitives::{utils::format_ether, Address, FixedBytes, TxKind, U256},
    rpc::types::TransactionRequest,
    signers::local::PrivateKeySigner,
};
use tracing::info;

use crate::{
    error::ContenderError,
    generator::seeder::{rand_seed::SeedGenerator, SeedValue},
};

pub trait SignerRegistry<Index: Ord> {
    fn get_signer(&self, idx: Index) -> Option<&PrivateKeySigner>;
    fn get_address(&self, idx: Index) -> Option<Address>;
}

pub trait AgentRegistry<Index: Ord> {
    fn get_agent(&self, idx: Index) -> Option<&Address>;
}

#[derive(Clone, Debug, Default)]
pub struct SignerStore {
    pub signers: Vec<PrivateKeySigner>,
}

#[derive(Clone, Debug)]
pub struct AgentStore {
    agents: HashMap<String, SignerStore>,
}

impl Default for AgentStore {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentStore {
    pub fn new() -> Self {
        AgentStore {
            agents: HashMap::new(),
        }
    }

    pub fn init(
        &mut self,
        agent_names: &[impl AsRef<str>],
        signers_per_agent: usize,
        seed: &impl SeedGenerator,
    ) {
        for agent in agent_names {
            if self.has_agent(agent) {
                continue;
            }
            self.add_new_agent(agent, signers_per_agent, seed);
        }
    }

    pub fn add_agent(&mut self, name: impl AsRef<str>, signers: SignerStore) {
        self.agents.insert(name.as_ref().to_owned(), signers);
    }

    pub fn add_new_agent(
        &mut self,
        name: impl AsRef<str>,
        num_signers: usize,
        rand_seeder: &impl SeedGenerator,
    ) {
        let signers = SignerStore::new(num_signers, rand_seeder, name.as_ref());
        self.add_agent(name, signers);
    }

    pub fn get_agent(&self, name: impl AsRef<str>) -> Option<&SignerStore> {
        self.agents.get(name.as_ref())
    }

    pub fn all_agents(&self) -> impl Iterator<Item = (&String, &SignerStore)> {
        self.agents.iter()
    }

    pub fn has_agent(&self, name: impl AsRef<str>) -> bool {
        self.agents.contains_key(name.as_ref())
    }

    pub fn remove_agent(&mut self, name: impl AsRef<str>) {
        self.agents.remove(name.as_ref());
    }

    pub fn all_signers(&self) -> Vec<&PrivateKeySigner> {
        self.agents
            .values()
            .flat_map(|s| s.signers.iter())
            .collect()
    }

    pub fn all_signer_addresses(&self) -> Vec<Address> {
        self.all_signers().iter().map(|s| s.address()).collect()
    }
}

impl<Idx> SignerRegistry<Idx> for SignerStore
where
    Idx: Ord + Into<usize>,
{
    fn get_signer(&self, idx: Idx) -> Option<&PrivateKeySigner> {
        self.signers.get::<usize>(idx.into())
    }

    fn get_address(&self, idx: Idx) -> Option<Address> {
        self.signers.get::<usize>(idx.into()).map(|s| s.address())
    }
}

impl SignerStore {
    pub fn new<S: SeedGenerator>(num_signers: usize, rand_seeder: &S, acct_seed: &str) -> Self {
        // add numerical value of acct_seed to given seed
        let new_seed = rand_seeder.as_u256() + U256::from_be_slice(acct_seed.as_bytes());
        let rand_seeder = S::seed_from_u256(new_seed);

        // generate random private keys with new seed
        let prv_keys = rand_seeder
            .seed_values(num_signers, None, None)
            .map(|sv| sv.as_bytes().to_vec())
            .collect::<Vec<_>>();
        let signers: Vec<PrivateKeySigner> = prv_keys
            .into_iter()
            .map(|s| FixedBytes::from_slice(&s))
            .map(|b| PrivateKeySigner::from_bytes(&b).expect("Failed to create random seed signer"))
            .collect();
        SignerStore { signers }
    }

    pub fn add_signer(&mut self, signer: PrivateKeySigner) {
        self.signers.push(signer);
    }

    pub fn remove_signer(&mut self, idx: usize) {
        self.signers.remove(idx);
    }

    pub fn all_addresses(&self) -> Vec<Address> {
        self.signers.iter().map(|s| s.address()).collect()
    }

    pub async fn fund_signers(
        &self,
        funder: &PrivateKeySigner,
        amount: U256,
        provider: impl alloy::providers::Provider<AnyNetwork> + 'static,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let gas_price = provider.get_gas_price().await?;
        // boost gas price by 10% to ensure the transaction is processed quickly
        let gas_price = gas_price * 11 / 10;
        let mut nonce = provider.get_transaction_count(funder.address()).await?;
        let funder_wallet = EthereumWallet::new(funder.to_owned());
        let provider = Arc::new(provider);
        let signers = self.all_addresses();
        let chain_id = provider.get_chain_id().await?;

        // create transaction request for each signer
        let tx_requests = signers
            .into_iter()
            .map(|address| {
                let tx = TransactionRequest::default()
                    .to(address)
                    .value(amount)
                    .from(funder.address())
                    .nonce(nonce)
                    .gas_limit(21000)
                    .max_fee_per_gas(gas_price)
                    .with_chain_id(chain_id)
                    .max_priority_fee_per_gas(gas_price / 10);

                nonce += 1;
                tx
            })
            .collect::<Vec<_>>();

        // sign and send txs
        let mut handles = vec![];
        for tx_request in tx_requests {
            let provider = provider.clone();
            let funder_wallet = funder_wallet.clone();
            let to_addr = match tx_request.to_owned().to.unwrap_or_default() {
                TxKind::Call(addr) => addr,
                TxKind::Create => Address::ZERO,
            };

            handles.push(tokio::task::spawn(async move {
                let signed_tx = tx_request
                    .build(&funder_wallet)
                    .await
                    .map_err(|e| ContenderError::with_err(e, "failed to sign funding tx"))?;
                let tx_hash = provider
                    .send_tx_envelope(AnyTxEnvelope::Ethereum(signed_tx))
                    .await
                    .map_err(|e| ContenderError::with_err(e, "failed to send funding tx"))?
                    .with_required_confirmations(1)
                    .watch()
                    .await
                    .map_err(|e| {
                        ContenderError::with_err(e, "funding tx failed to land onchain")
                    })?;
                info!(
                    "Funded {to_addr} with {} ether, ({tx_hash})",
                    format_ether(amount)
                );
                Ok::<_, ContenderError>(())
            }));
            // Sleep to avoid overwhelming the provider with requests
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        for handle in handles {
            handle.await??
        }

        Ok(())
    }
}
