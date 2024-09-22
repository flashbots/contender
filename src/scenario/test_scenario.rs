use crate::db::database::DbOps;
use crate::error::ContenderError;
use crate::generator::{
    seeder::Seeder,
    types::{PlanType, TestConfig},
    Generator2, PlanConfig,
};
use alloy::hex::ToHexExt;
use alloy::network::{EthereumWallet, TransactionBuilder};
use alloy::primitives::Address;
use alloy::providers::{Provider, ProviderBuilder};
use alloy::signers::local::PrivateKeySigner;
use alloy::transports::http::reqwest::Url;
use std::collections::HashMap;
use std::sync::Arc;

/// A test scenario can be used to run a test with a specific configuration, database, and RPC provider.
pub struct TestScenario<D, S>
where
    D: DbOps + Send + Sync + 'static,
    S: Seeder,
{
    pub config: TestConfig,
    db: Arc<D>,
    pub rpc_url: Url,
    pub rand_seed: S,
    pub wallet_map: HashMap<Address, EthereumWallet>,
}

impl<D, S> TestScenario<D, S>
where
    D: DbOps + Send + Sync + 'static,
    S: Seeder + Send + Sync,
{
    pub fn new(
        config: TestConfig,
        db: D,
        rpc_url: Url,
        rand_seed: S,
        signers: &[PrivateKeySigner],
    ) -> Self {
        let mut wallet_map = HashMap::new();
        let wallets = signers.iter().map(|s| {
            let w = EthereumWallet::new(s.clone());
            (s.address(), w)
        });
        for (addr, wallet) in wallets {
            wallet_map.insert(addr, wallet);
        }

        Self {
            config,
            db: Arc::new(db),
            rpc_url,
            rand_seed,
            wallet_map,
        }
    }

    pub async fn deploy_contracts(
        &self,
        // only_with_names: Option<&[impl AsRef<str>]>,
    ) -> Result<(), ContenderError> {
        let pub_provider = ProviderBuilder::new().on_http(self.rpc_url.clone());
        let gas_price = pub_provider.get_gas_price().await.map_err(|e| {
            ContenderError::SetupError("failed to get gas price", Some(e.to_string()))
        })?;

        // we do everything in the callback so no need to actually capture the returned txs
        self.load_txs(PlanType::Create(|tx_req| {
            /* callback */
            // copy data/refs from self before spawning the task
            let db = self.db.clone();
            let from = tx_req.tx.from.as_ref().ok_or(ContenderError::SetupError(
                "failed to get 'from' address",
                None,
            ))?;
            let wallet = self
                .wallet_map
                .get(from)
                .expect(&format!("couldn't find wallet for 'from' address {}", from))
                .to_owned();
            let provider = ProviderBuilder::new()
                // simple_nonce_management is unperformant but it's OK bc we're just deploying
                .with_simple_nonce_management()
                .wallet(wallet)
                .on_http(self.rpc_url.to_owned());

            println!("deploying contract: {:?}", tx_req.name);
            let handle = tokio::task::spawn(async move {
                // estimate gas limit
                let gas_limit = provider
                    .estimate_gas(&tx_req.tx)
                    .await
                    .expect("failed to estimate gas");

                // inject missing fields into tx_req.tx
                let tx = tx_req
                    .tx
                    .with_gas_price(gas_price)
                    .with_gas_limit(gas_limit);

                let res = provider.send_transaction(tx).await.unwrap();
                let receipt = res.get_receipt().await.unwrap();
                println!("contract address: {:?}", receipt.contract_address);
                let contract_address = receipt.contract_address;
                db.insert_named_tx(
                    tx_req.name.unwrap_or_default(),
                    receipt.transaction_hash,
                    contract_address,
                )
                .expect("failed to insert tx into db");
            });
            Ok(Some(handle))
        }))
        .await?;

        Ok(())
    }

    pub async fn run_setup(&self) -> Result<(), ContenderError> {
        self.load_txs(PlanType::Setup(|tx_req| {
            /* callback */
            // copy data/refs from self before spawning the task
            let from = tx_req.tx.from.as_ref().ok_or(ContenderError::SetupError(
                "failed to get 'from' address",
                None,
            ))?;
            println!("from: {:?}", from);
            let wallet = self
                .wallet_map
                .get(from)
                .ok_or(ContenderError::SetupError(
                    "couldn't find private key for address",
                    from.encode_hex().into(),
                ))?
                .to_owned();
            let db = self.db.clone();
            let provider = ProviderBuilder::new()
                .with_simple_nonce_management()
                .wallet(wallet)
                .on_http(self.rpc_url.to_owned());

            println!("running setup: {:?}", tx_req.name);
            let handle = tokio::task::spawn(async move {
                let gas_price = provider.get_gas_price().await.unwrap();
                let gas_limit = provider
                    .estimate_gas(&tx_req.tx)
                    .await
                    .expect("failed to estimate gas");
                let tx = tx_req
                    .tx
                    .with_gas_price(gas_price)
                    .with_gas_limit(gas_limit);
                let res = provider.send_transaction(tx).await.unwrap();
                let receipt = res.get_receipt().await.unwrap();
                if let Some(name) = tx_req.name {
                    db.insert_named_tx(name, receipt.transaction_hash, receipt.contract_address)
                        .expect("failed to insert tx into db");
                }
            });
            Ok(Some(handle))
        }))
        .await?;

        Ok(())
    }
}

impl<D, S> Generator2<String, D, TestConfig> for TestScenario<D, S>
where
    D: DbOps + Send + Sync,
    S: Seeder,
{
    fn get_db(&self) -> &D {
        self.db.as_ref()
    }

    fn get_templater(&self) -> &TestConfig {
        &self.config
    }

    fn get_plan_conf(&self) -> &impl PlanConfig<String> {
        &self.config
    }

    fn get_fuzz_seeder(&self) -> &impl Seeder {
        &self.rand_seed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::sqlite::SqliteDb;
    use crate::generator::testfile::tests::{get_composite_testconfig, get_test_signers};
    use crate::generator::{types::PlanType, util::test::spawn_anvil, RandSeed};
    use crate::scenario::test_scenario::TestScenario;
    use alloy::node_bindings::AnvilInstance;

    fn get_test_scenario(anvil: &AnvilInstance) -> TestScenario<SqliteDb, RandSeed> {
        let test_file = get_composite_testconfig();
        let seed = RandSeed::from_bytes(&[0x01; 32]);
        let db = SqliteDb::new_memory();
        db.create_tables().unwrap();
        let signers = &get_test_signers();

        TestScenario::new(test_file, db, anvil.endpoint_url(), seed, &signers)
    }

    #[tokio::test]
    async fn it_creates_scenarios() {
        let anvil = spawn_anvil();
        let scenario = get_test_scenario(&anvil);

        let create_txs = scenario
            .load_txs(PlanType::Create(|tx| {
                println!("create tx callback triggered! {:?}\n", tx);
                Ok(None)
            }))
            .await
            .unwrap();
        assert_eq!(create_txs.len(), 1);

        let setup_txs = scenario
            .load_txs(PlanType::Setup(|tx| {
                println!("setup tx callback triggered! {:?}\n", tx);
                Ok(None)
            }))
            .await
            .unwrap();
        assert_eq!(setup_txs.len(), 2);

        let spam_txs = scenario
            .load_txs(PlanType::Spam(20, |tx| {
                println!("spam tx callback triggered! {:?}\n", tx);
                Ok(None)
            }))
            .await
            .unwrap();
        assert!(spam_txs.len() >= 20);
    }

    #[tokio::test]
    async fn scenario_creates_contracts() {
        let anvil = spawn_anvil();
        let scenario = get_test_scenario(&anvil);
        let res = scenario.deploy_contracts().await;
        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn scenario_runs_setup() {
        let anvil = spawn_anvil();
        let scenario = get_test_scenario(&anvil);
        scenario.deploy_contracts().await.unwrap();
        let res = scenario.run_setup().await;
        println!("{:?}", res);
        assert!(res.is_ok());
    }
}
