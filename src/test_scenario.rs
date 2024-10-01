use crate::db::{DbOps, NamedTx};
use crate::error::ContenderError;
use crate::generator::templater::Templater;
use crate::generator::{seeder::Seeder, types::PlanType, Generator, PlanConfig};
use alloy::hex::ToHexExt;
use alloy::network::{EthereumWallet, TransactionBuilder};
use alloy::primitives::Address;
use alloy::providers::{Provider, ProviderBuilder};
use alloy::signers::local::PrivateKeySigner;
use alloy::transports::http::reqwest::Url;
use std::collections::HashMap;
use std::sync::Arc;

/// A test scenario can be used to run a test with a specific configuration, database, and RPC provider.
pub struct TestScenario<D, S, P>
where
    D: DbOps + Send + Sync + 'static,
    S: Seeder,
    P: PlanConfig<String> + Templater<String> + Send + Sync,
{
    pub config: P,
    pub db: Arc<D>,
    pub rpc_url: Url,
    pub rand_seed: S,
    pub wallet_map: HashMap<Address, EthereumWallet>,
}

impl<D, S, P> TestScenario<D, S, P>
where
    D: DbOps + Send + Sync + 'static,
    S: Seeder + Send + Sync,
    P: PlanConfig<String> + Templater<String> + Send + Sync,
{
    pub fn new(
        config: P,
        db: Arc<D>,
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
            db: db.clone(),
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
        let gas_price = pub_provider
            .get_gas_price()
            .await
            .map_err(|e| ContenderError::with_err(e, "failed to get gas price"))?;
        let chain_id = pub_provider
            .get_chain_id()
            .await
            .map_err(|e| ContenderError::with_err(e, "failed to get chain id"))?;

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
                    .with_chain_id(chain_id)
                    .with_gas_limit(gas_limit);

                let res = provider
                    .send_transaction(tx)
                    .await
                    .expect("failed to send tx");
                let receipt = res.get_receipt().await.expect("failed to get receipt");
                println!("contract address: {:?}", receipt.contract_address);
                let contract_address = receipt.contract_address;
                db.insert_named_txs(
                    NamedTx::new(
                        tx_req.name.unwrap_or_default(),
                        receipt.transaction_hash,
                        contract_address,
                    )
                    .into(),
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
                let chain_id = provider
                    .get_chain_id()
                    .await
                    .expect("failed to get chain id");
                let gas_price = provider
                    .get_gas_price()
                    .await
                    .expect("failed to get gas price");
                let gas_limit = provider
                    .estimate_gas(&tx_req.tx)
                    .await
                    .expect("failed to estimate gas");
                let tx = tx_req
                    .tx
                    .with_gas_price(gas_price)
                    .with_chain_id(chain_id)
                    .with_gas_limit(gas_limit);
                let res = provider
                    .send_transaction(tx)
                    .await
                    .expect("failed to send tx");
                let receipt = res.get_receipt().await.expect("failed to get receipt");
                if let Some(name) = tx_req.name {
                    db.insert_named_txs(
                        NamedTx::new(name, receipt.transaction_hash, receipt.contract_address)
                            .into(),
                    )
                    .expect("failed to insert tx into db");
                }
            });
            Ok(Some(handle))
        }))
        .await?;

        Ok(())
    }
}

impl<D, S, P> Generator<String, D, P> for TestScenario<D, S, P>
where
    D: DbOps + Send + Sync,
    S: Seeder,
    P: PlanConfig<String> + Templater<String> + Send + Sync,
{
    fn get_db(&self) -> &D {
        self.db.as_ref()
    }

    fn get_templater(&self) -> &P {
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
pub mod tests {
    use crate::db::MockDb;
    use crate::generator::templater::Templater;
    use crate::generator::types::{CreateDefinition, FunctionCallDefinition, FuzzParam};
    use crate::generator::{types::PlanType, util::test::spawn_anvil, RandSeed};
    use crate::generator::{Generator, PlanConfig};
    use crate::spammer::util::test::get_test_signers;
    use crate::test_scenario::TestScenario;
    use crate::Result;
    use alloy::hex::ToHexExt;
    use alloy::node_bindings::AnvilInstance;
    use alloy::primitives::Address;
    use std::collections::HashMap;

    pub struct MockConfig;

    pub const COUNTER_BYTECODE: &'static str =
        "0x608060405234801561001057600080fd5b5060f78061001f6000396000f3fe6080604052348015600f57600080fd5b5060043610603c5760003560e01c80633fb5c1cb1460415780638381f58a146053578063d09de08a14606d575b600080fd5b6051604c3660046083565b600055565b005b605b60005481565b60405190815260200160405180910390f35b6051600080549080607c83609b565b9190505550565b600060208284031215609457600080fd5b5035919050565b60006001820160ba57634e487b7160e01b600052601160045260246000fd5b506001019056fea264697066735822122010f3077836fb83a22ad708a23102f2b487523767e1afef5a93c614619001648b64736f6c63430008170033";

    impl PlanConfig<String> for MockConfig {
        fn get_env(&self) -> Result<HashMap<String, String>> {
            Ok(HashMap::<String, String>::from_iter([
                ("test1".to_owned(), "0xbeef".to_owned()),
                ("test2".to_owned(), "0x9001".to_owned()),
            ]))
        }

        fn get_create_steps(&self) -> Result<Vec<CreateDefinition>> {
            Ok(vec![CreateDefinition {
                bytecode: COUNTER_BYTECODE.to_string(),
                name: "test_counter".to_string(),
                from: "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266".to_owned(),
            }])
        }

        fn get_setup_steps(&self) -> Result<Vec<FunctionCallDefinition>> {
            Ok(vec![
                FunctionCallDefinition {
                    to: "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D".to_owned(),
                    from: "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266".to_owned(),
                    value: Some("4096".to_owned()),
                    signature: "swap(uint256 x, uint256 y, address a, bytes b)".to_owned(),
                    args: vec![
                        "1".to_owned(),
                        "2".to_owned(),
                        Address::repeat_byte(0x11).encode_hex(),
                        "0xdead".to_owned(),
                    ]
                    .into(),
                    fuzz: None,
                },
                FunctionCallDefinition {
                    to: "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D".to_owned(),
                    from: "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266".to_owned(),
                    value: Some("0x1000".to_owned()),
                    signature: "swap(uint256 x, uint256 y, address a, bytes b)".to_owned(),
                    args: vec![
                        "1".to_owned(),
                        "2".to_owned(),
                        Address::repeat_byte(0x11).encode_hex(),
                        "0xbeef".to_owned(),
                    ]
                    .into(),
                    fuzz: None,
                },
            ])
        }

        fn get_spam_steps(&self) -> Result<Vec<FunctionCallDefinition>> {
            let fn_call = |data: &str, from_addr: &str| FunctionCallDefinition {
                to: "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D".to_owned(),
                from: from_addr.to_owned(),
                value: None,
                signature: "swap(uint256 x, uint256 y, address a, bytes b)".to_owned(),
                args: vec![
                    "1".to_owned(),
                    "2".to_owned(),
                    Address::repeat_byte(0x11).encode_hex(),
                    data.to_owned(),
                ]
                .into(),
                fuzz: vec![FuzzParam {
                    param: "x".to_string(),
                    min: None,
                    max: None,
                }]
                .into(),
            };
            Ok(vec![
                fn_call("0xbeef", "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"),
                fn_call("0xea75", "0x70997970C51812dc3A010C7d01b50e0d17dc79C8"),
                fn_call("0xf00d", "0x3C44CdDdB6a900fa2b585dd299e03d12FA4293BC"),
            ])
        }
    }

    impl Templater<String> for MockConfig {
        fn copy_end(&self, input: &str, _last_end: usize) -> String {
            input.to_owned()
        }
        fn replace_placeholders(
            &self,
            input: &str,
            _placeholder_map: &std::collections::HashMap<String, String>,
        ) -> String {
            input.to_owned()
        }
        fn terminator_start(&self, _input: &str) -> Option<usize> {
            None
        }
        fn terminator_end(&self, _input: &str) -> Option<usize> {
            None
        }
        fn num_placeholders(&self, _input: &str) -> usize {
            0
        }
        fn find_key(&self, _input: &str) -> Option<(String, usize)> {
            None
        }
        fn encode_contract_address(&self, input: &Address) -> String {
            input.encode_hex()
        }
    }

    pub fn get_test_scenario(anvil: &AnvilInstance) -> TestScenario<MockDb, RandSeed, MockConfig> {
        let seed = RandSeed::from_bytes(&[0x01; 32]);
        let signers = &get_test_signers();

        TestScenario::new(
            MockConfig,
            MockDb.into(),
            anvil.endpoint_url(),
            seed,
            &signers,
        )
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
