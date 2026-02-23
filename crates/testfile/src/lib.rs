mod campaign;
pub mod error;
mod test_config;

pub use campaign::{
    CampaignConfig, CampaignMixEntry, CampaignMode, CampaignSpam, CampaignStage, ResolvedMixEntry,
    ResolvedStage,
};
pub use error::Error;
pub use test_config::TestConfig;

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
pub mod tests {
    use super::TestConfig;
    use alloy::{
        consensus::TxType,
        hex::ToHexExt,
        node_bindings::{Anvil, AnvilInstance, WEI_IN_ETHER},
        primitives::{Address, U256},
        signers::local::PrivateKeySigner,
    };
    use contender_core::generator::{
        templater::Templater, BundleCallDefinition, CompiledContract, CreateDefinition,
    };
    use contender_core::{
        db::MockDb,
        generator::{
            named_txs::ExecutionRequest,
            types::{PlanType, SpamRequest},
            FunctionCallDefinition, FuzzParam, Generator, RandSeed,
        },
        test_scenario::{TestScenario, TestScenarioParams},
    };
    use std::{collections::HashMap, fs, str::FromStr};
    use tokio::sync::OnceCell;

    // prometheus
    static PROM: OnceCell<prometheus::Registry> = OnceCell::const_new();
    static HIST: OnceCell<prometheus::HistogramVec> = OnceCell::const_new();

    pub fn spawn_anvil() -> AnvilInstance {
        Anvil::new().block_time_f64(0.25).try_spawn().unwrap()
    }

    pub const COUNTER_BYTECODE: &str =
        "0x608060405234801561001057600080fd5b5060f78061001f6000396000f3fe6080604052348015600f57600080fd5b5060043610603c5760003560e01c80633fb5c1cb1460415780638381f58a146053578063d09de08a14606d575b600080fd5b6051604c3660046083565b600055565b005b605b60005481565b60405190815260200160405180910390f35b6051600080549080607c83609b565b9190505550565b600060208284031215609457600080fd5b5035919050565b60006001820160ba57634e487b7160e01b600052601160045260246000fd5b506001019056fea264697066735822122010f3077836fb83a22ad708a23102f2b487523767e1afef5a93c614619001648b64736f6c63430008170033";

    pub fn get_test_signers() -> Vec<PrivateKeySigner> {
        [
            "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
            "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d",
            "0x5de4111afa1a4b94908f83103eb1f1706367c2e68ca870fc3fb9a804cdab365a",
        ]
        .iter()
        .map(|s| PrivateKeySigner::from_str(s).unwrap())
        .collect::<Vec<PrivateKeySigner>>()
    }

    pub fn get_testconfig() -> TestConfig {
        let fncall = FunctionCallDefinition::new("0x7a250d5630B4cF539739dF2C5dAcb4c659F248DD")
            .with_from("0x7a250d5630B4cF539739dF2C5dAcb4c659F248DD")
            .with_signature("swap(uint256 x, uint256 y, address a, bytes b)")
            .with_args(&[
                "1".to_owned(),
                "2".to_owned(),
                Address::repeat_byte(0x11).encode_hex(),
                "0xdead".to_owned(),
            ]);

        TestConfig {
            env: None,
            create: None,
            setup: None,
            spam: vec![SpamRequest::Tx(Box::new(fncall))].into(),
        }
    }

    pub fn get_fuzzy_testconfig() -> TestConfig {
        let fn_call = |data: &str, from_addr: &str| {
            FunctionCallDefinition::new("0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D")
                .with_from(from_addr)
                .with_signature("swap(uint256 x, uint256 y, address a, bytes b)")
                .with_args(&[
                    "1".to_owned(),
                    "2".to_owned(),
                    Address::repeat_byte(0x11).encode_hex(),
                    data.to_owned(),
                ])
                .with_fuzz(&[FuzzParam {
                    param: Some("x".to_string()),
                    value: None,
                    min: None,
                    max: None,
                }])
        };
        TestConfig {
            env: None,
            create: None,
            setup: None,
            spam: vec![
                SpamRequest::Tx(Box::new(fn_call(
                    "0xbeef",
                    "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266",
                ))),
                SpamRequest::Tx(Box::new(fn_call(
                    "0xea75",
                    "0x70997970C51812dc3A010C7d01b50e0d17dc79C8",
                ))),
                SpamRequest::Tx(Box::new(fn_call(
                    "0xf00d",
                    "0x3C44CdDdB6a900fa2b585dd299e03d12FA4293BC",
                ))),
                SpamRequest::Bundle(BundleCallDefinition {
                    txs: vec![
                        fn_call("0xbeef", "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"),
                        fn_call("0xea75", "0x70997970C51812dc3A010C7d01b50e0d17dc79C8"),
                        fn_call("0xf00d", "0x3C44CdDdB6a900fa2b585dd299e03d12FA4293BC"),
                    ],
                }),
            ]
            .into(),
        }
    }

    pub fn get_setup_testconfig() -> TestConfig {
        TestConfig {
            env: None,
            create: None,
            spam: None,
            setup: vec![
                FunctionCallDefinition::new("0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D")
                    .with_from("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266")
                    .with_value(U256::from(4096))
                    .with_signature("swap(uint256 x, uint256 y, address a, bytes b)")
                    .with_args(&["1", "2", &Address::repeat_byte(0x11).encode_hex(), "0xdead"]),
                FunctionCallDefinition::new("0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D")
                    .with_from("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266")
                    .with_value(U256::from(0x1000))
                    .with_signature("swap(uint256 x, uint256 y, address a, bytes b)")
                    .with_args(&["1", "2", &Address::repeat_byte(0x11).encode_hex(), "0xbeef"]),
            ]
            .into(),
        }
    }

    pub fn get_create_testconfig() -> TestConfig {
        let mut env = HashMap::new();
        env.insert("test1".to_owned(), "0xbeef".to_owned());
        env.insert("test2".to_owned(), "0x9001".to_owned());
        TestConfig {
            env: Some(env),
            create: Some(vec![CreateDefinition {
                contract: CompiledContract::new(
                    COUNTER_BYTECODE.to_string(),
                    "test_counter".to_string(),
                ),
                signature: None,
                args: None,
                from: Some("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266".to_owned()),
                from_pool: None,
            }]),
            spam: None,
            setup: None,
        }
    }

    pub fn get_composite_testconfig() -> TestConfig {
        let tc_fuzz = get_fuzzy_testconfig();
        let tc_setup = get_setup_testconfig();
        let tc_create = get_create_testconfig();
        TestConfig {
            env: tc_create.env, // TODO: add something here
            create: tc_create.create,
            spam: tc_fuzz.spam,
            setup: tc_setup.setup,
        }
    }

    #[tokio::test]
    async fn parses_testconfig_toml() {
        let test_file = TestConfig::from_file("testConfig.toml").unwrap();
        assert!(test_file.env.is_some());
        assert!(test_file.setup.is_some());
        assert!(test_file.spam.is_some());
        let env = test_file.env.unwrap();
        let setup = test_file.setup.unwrap();
        let spam = test_file.spam.unwrap();

        assert_eq!(env.get("env1").unwrap(), "env1");
        match spam[0] {
            SpamRequest::Tx(ref fncall) => {
                assert_eq!(
                    *fncall.from.as_ref().unwrap(),
                    "0x70997970C51812dc3A010C7d01b50e0d17dc79C8".to_owned()
                );
                assert_eq!(setup.len(), 1);
                assert_eq!(setup[0].value, Some("1234".to_owned()));
                assert_eq!(
                    fncall.fuzz.as_ref().unwrap()[0].param.to_owned().unwrap(),
                    "amountIn"
                );
                assert_eq!(fncall.fuzz.as_ref().unwrap()[0].min, Some(U256::from(1)));
                assert_eq!(
                    fncall.fuzz.as_ref().unwrap()[0].max,
                    Some(U256::from(100_000_000_000_000_000_u64))
                );
                assert_eq!(fncall.kind, Some("test".to_owned()));
            }
            _ => {
                panic!("expected SpamRequest::Single");
            }
        }
    }

    fn repo_root_path() -> std::path::PathBuf {
        let mut dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        dir.pop(); // crates
        dir.pop(); // repo root
        dir
    }

    fn collect_scenario_tomls(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut stack = vec![dir.to_path_buf()];
        let mut files = Vec::new();
        while let Some(p) = stack.pop() {
            for entry in std::fs::read_dir(&p).unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().and_then(|s| s.to_str()) == Some("toml") {
                    files.push(path);
                }
            }
        }
        files
    }

    #[test]
    fn parses_all_repo_scenarios() {
        let repo_root = repo_root_path();
        let scenarios_dir = repo_root.join("scenarios");
        assert!(
            scenarios_dir.exists(),
            "scenarios/ directory not found at {}",
            scenarios_dir.display()
        );

        let files = collect_scenario_tomls(&scenarios_dir);
        for path in files {
            TestConfig::from_file(path.to_str().unwrap()).unwrap();
        }
    }

    fn print_testconfig(cfg: &str) {
        println!("{}", "-".repeat(80));
        println!("{cfg}");
        println!("{}", "-".repeat(80));
    }

    #[test]
    fn encodes_testconfig_toml() {
        let cfg = get_composite_testconfig();
        let encoded = cfg.encode_toml().unwrap();
        print_testconfig(&encoded);
        cfg.save_toml("cargotest.toml").unwrap();
        let test_file2 = TestConfig::from_file("cargotest.toml").unwrap();
        let spam = cfg.clone().spam.unwrap();
        match &spam[0] {
            SpamRequest::Tx(req) => {
                let args = req.args.as_ref().unwrap();
                match &test_file2.spam.unwrap()[0] {
                    SpamRequest::Tx(req2) => {
                        let args2 = req2.args.as_ref().unwrap();
                        assert_eq!(req.from, req2.from);
                        assert_eq!(req.to, req2.to);
                        assert_eq!(args[0], args2[0]);
                        assert_eq!(args[1], args2[1]);
                    }
                    _ => {
                        panic!("expected SpamRequest::Single");
                    }
                }
            }
            _ => {
                panic!("expected SpamRequest::Single");
            }
        }
        fs::remove_file("cargotest.toml").unwrap();
    }

    #[tokio::test]
    async fn gets_spam_txs() {
        let anvil = spawn_anvil();
        let test_file = get_testconfig();
        let seed = RandSeed::new();
        let tx_type = TxType::Eip1559;
        let test_gen = TestScenario::new(
            test_file,
            MockDb.into(),
            seed,
            TestScenarioParams {
                rpc_url: anvil.endpoint_url(),
                builder_rpc_url: None,
                signers: get_test_signers(),
                agent_spec: Default::default(),
                tx_type,
                bundle_type: Default::default(),
                pending_tx_timeout_secs: 12,
                extra_msg_handles: None,
                sync_nonces_after_batch: true,
                rpc_batch_size: 0,
                gas_price: None,
                scenario_label: None,
                send_raw_tx_sync: false,
            },
            None,
            (&PROM, &HIST).into(),
        )
        .await
        .unwrap();
        // this seed can be used to recreate the same test tx(s)
        let (spam_txs, _nonces) = test_gen
            .load_txs(PlanType::Spam(10, |_tx_req| {
                println!(
                    "spam tx\n\tfrom={:?}\n\tto={:?}\n\tinput={:?}",
                    _tx_req.tx.from, _tx_req.tx.to, _tx_req.tx.input.input
                );
                Ok(None)
            }))
            .await
            .unwrap();
        assert_eq!(spam_txs.len(), 10);
        match &spam_txs[0] {
            ExecutionRequest::Tx(req) => {
                let data = req.tx.input.input.to_owned().unwrap().to_string();
                assert_eq!(data, "0x022c0d9f00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000002000000000000000000000000111111111111111111111111111111111111111100000000000000000000000000000000000000000000000000000000000000800000000000000000000000000000000000000000000000000000000000000002dead000000000000000000000000000000000000000000000000000000000000");
            }
            _ => {
                panic!("expected ExecutionRequest::Tx");
            }
        }
    }

    #[tokio::test]
    async fn fuzz_is_deterministic() {
        let anvil = spawn_anvil();
        let test_file = get_fuzzy_testconfig();
        let seed = RandSeed::seed_from_bytes(&[0x01; 32]);
        let signers = get_test_signers();
        let tx_type = TxType::Eip1559;
        let scenario1 = TestScenario::new(
            test_file.clone(),
            MockDb.into(),
            seed.to_owned(),
            TestScenarioParams {
                rpc_url: anvil.endpoint_url(),
                builder_rpc_url: None,
                signers: signers.to_owned(),
                agent_spec: Default::default(),
                tx_type,
                bundle_type: Default::default(),
                pending_tx_timeout_secs: 12,
                extra_msg_handles: None,
                sync_nonces_after_batch: true,
                rpc_batch_size: 0,
                gas_price: None,
                scenario_label: None,
                send_raw_tx_sync: false,
            },
            None,
            (&PROM, &HIST).into(),
        )
        .await
        .unwrap();
        let scenario2 = TestScenario::new(
            test_file,
            MockDb.into(),
            seed,
            TestScenarioParams {
                rpc_url: anvil.endpoint_url(),
                builder_rpc_url: None,
                signers,
                agent_spec: Default::default(),
                tx_type,
                bundle_type: Default::default(),
                pending_tx_timeout_secs: 12,
                extra_msg_handles: None,
                sync_nonces_after_batch: true,
                rpc_batch_size: 0,
                gas_price: None,
                scenario_label: None,
                send_raw_tx_sync: false,
            },
            None,
            (&PROM, &HIST).into(),
        )
        .await
        .unwrap();

        let num_txs = 13;
        let (spam_txs_1, _nonces) = scenario1
            .load_txs(PlanType::Spam(num_txs, |_| Ok(None)))
            .await
            .unwrap();
        let (spam_txs_2, _nonces) = scenario2
            .load_txs(PlanType::Spam(num_txs, |_| Ok(None)))
            .await
            .unwrap();
        assert_eq!(spam_txs_1.len(), spam_txs_2.len());
        for i in 0..spam_txs_1.len() {
            match &spam_txs_1[i] {
                ExecutionRequest::Tx(req) => {
                    let data1 = req.tx.input.input.to_owned().unwrap().to_string();
                    match &spam_txs_2[i] {
                        ExecutionRequest::Tx(req) => {
                            let data2 = req.tx.input.input.to_owned().unwrap().to_string();
                            assert_eq!(data1, data2);
                        }
                        _ => {
                            panic!("expected ExecutionRequest::Tx");
                        }
                    }
                }
                ExecutionRequest::Bundle(reqs) => {
                    let data1 = reqs[0].tx.input.input.to_owned().unwrap().to_string();
                    match &spam_txs_2[i] {
                        ExecutionRequest::Bundle(reqs) => {
                            let data2 = reqs[0].tx.input.input.to_owned().unwrap().to_string();
                            assert_eq!(data1, data2);
                        }
                        _ => {
                            panic!("expected ExecutionRequest::Bundle");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn test_placeholders_count() {
        let test_config = TestConfig::default();

        let count = test_config.num_placeholders("{lol}{baa}{hahaa}");

        assert_eq!(count, 3);
    }

    #[test]
    fn test_placeholders_find() {
        let test_config = TestConfig::default();

        let mut placeholder_map = HashMap::new();
        test_config
            .find_placeholder_values(
                "{lol}{baa}{hahaa}",
                &mut placeholder_map,
                &MockDb,
                "http://localhost:8545",
                Default::default(),
                None,
            )
            .unwrap();

        assert_eq!(placeholder_map.len(), 3);
    }

    const CONFIG_FILE_WITH_PLACEHOLDERS: &str = "
[env]
mySender = \"0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266\"
[[spam]]
[spam.tx]
from = \"{mySender}\"
to = \"{_sender}\"
value = \"1eth\"
";

    fn default_scenario_params(anvil: &AnvilInstance) -> TestScenarioParams {
        TestScenarioParams {
            rpc_url: anvil.endpoint_url(),
            builder_rpc_url: None,
            signers: get_test_signers(),
            agent_spec: Default::default(),
            tx_type: Default::default(),
            bundle_type: Default::default(),
            pending_tx_timeout_secs: 12,
            extra_msg_handles: None,
            sync_nonces_after_batch: true,
            rpc_batch_size: 0,
            gas_price: None,
            scenario_label: None,
            send_raw_tx_sync: false,
        }
    }

    async fn default_scenario(anvil: &AnvilInstance) -> TestScenario<MockDb, RandSeed, TestConfig> {
        TestScenario::<MockDb, RandSeed, TestConfig>::new(
            TestConfig::from_str(CONFIG_FILE_WITH_PLACEHOLDERS).unwrap(),
            MockDb.into(),
            RandSeed::default(),
            default_scenario_params(anvil),
            None,
            (&PROM, &HIST).into(),
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn placeholders_work_in_from_field() {
        let anvil = Anvil::new().spawn();
        let scenario = default_scenario(&anvil).await;

        let spam = scenario.get_spam_tx_chunks(1, 1).await.unwrap();
        for tx in &spam[0] {
            match tx {
                ExecutionRequest::Tx(tx) => {
                    assert_eq!(
                        tx.tx.from.unwrap(),
                        "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"
                            .parse::<Address>()
                            .unwrap()
                    );
                }
                ExecutionRequest::Bundle(_) => {
                    panic!("there should be no bundles in this config");
                }
            }
        }
    }

    #[tokio::test]
    async fn fncall_value_accepts_units_or_wei() -> Result<(), Box<dyn std::error::Error>> {
        let anvil = Anvil::new().spawn();
        let mut scenario = default_scenario(&anvil).await;

        // change the spam directive's `value` field to show more supported styles
        for val in [
            "1 eth",
            "1eth",
            "1 ether",
            "1000000000000000000",
            "1000000000 gwei",
        ] {
            // update the [[spam]] directive we specify in our TOML config
            let mut spam_directives = scenario.config.spam.to_owned().unwrap();
            match &spam_directives[0] {
                SpamRequest::Tx(tx) => {
                    let mut new_tx = tx.to_owned();
                    new_tx.value = Some(val.to_owned());
                    spam_directives = vec![SpamRequest::Tx(new_tx)];
                }
                SpamRequest::Bundle(_) => {
                    panic!("this should not be a bundle");
                }
            }
            println!("spam tx: {:#?}", spam_directives[0]);
            scenario.config.spam = Some(spam_directives);

            // check spam TxRequests generated by contender
            let spam_chunks = scenario.get_spam_tx_chunks(1, 1).await?;
            for tx in &spam_chunks[0] {
                match tx {
                    ExecutionRequest::Tx(tx) => {
                        assert_eq!(tx.tx.value, Some(WEI_IN_ETHER));
                    }
                    ExecutionRequest::Bundle(_) => {
                        panic!("there should be no bundles in this config");
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod more_tests {
    use super::*;
    use alloy::node_bindings::Anvil;
    use contender_core::{
        db::MockDb,
        generator::{types::SpamRequest, FunctionCallDefinition, RandSeed},
        spammer::{NilCallback, TimedSpammer},
        Contender, ContenderCtx, RunOpts,
    };
    use std::time::Duration;

    #[tokio::test]
    async fn contender_ctx_builder_runs() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let anvil = Anvil::new().spawn();

        let config = TestConfig::new().with_spam(vec![SpamRequest::new_tx(
            &FunctionCallDefinition::new("{_sender}") // send tx to self
                .with_kind("cargo_test"),
        )]);

        let db = MockDb;
        let seeder = RandSeed::new();
        let ctx = ContenderCtx::builder(config, db, seeder, anvil.endpoint_url()).build();
        let mut contender = Contender::new(ctx);

        let spammer = TimedSpammer::new(Duration::from_secs(1));
        let callback = NilCallback;
        let opts = RunOpts::new().txs_per_period(100).periods(3);
        contender.spam(spammer, callback.into(), opts).await?;

        Ok(())
    }
}

#[cfg(test)]
mod campaign_tests {
    use super::{CampaignConfig, CampaignMode};

    #[test]
    fn parses_campaign_and_resolves_stages() {
        let toml = r#"
name = "composite"
description = "traffic mix of erc20 and groth16_verify"

[spam]
mode = "tps"
rate = 20
duration = 600
seed = 42

[[spam.stage]]
name = "steady"
duration_secs = 600
  [[spam.stage.mix]]
  scenario  = "scenario:other_contract_call.toml"
  share_pct = 95.0
  [[spam.stage.mix]]
  scenario  = "scenario:eth_transfer.toml"
  share_pct = 4.8
  [[spam.stage.mix]]
  scenario  = "scenario:erc20_transfer.toml"
  share_pct = 0.2
"#;

        let cfg = CampaignConfig::from_toml_str(toml).expect("campaign parses");
        let stages = cfg.resolve().expect("campaign resolves");
        assert_eq!(stages.len(), 1);
        let stage = &stages[0];
        assert_eq!(cfg.spam.mode, CampaignMode::Tps);
        assert_eq!(stage.rate, 20);
        assert_eq!(stage.duration, 600);
        assert_eq!(stage.mix.len(), 3);
        let total: u64 = stage.mix.iter().map(|m| m.rate).sum();
        assert_eq!(total, 20);
        assert!(stage
            .mix
            .iter()
            .any(|m| m.scenario.contains("erc20_transfer")));
    }

    mod scenario_label {
        use crate::TestConfig;
        use alloy::primitives::Address;
        use contender_core::db::{DbOps, NamedTx};
        use contender_core::generator::templater::Templater;
        use contender_sqlite::SqliteDb;
        use std::collections::HashMap;

        fn setup_db_with_named_tx(name: &str) -> SqliteDb {
            let db = SqliteDb::new_memory();
            db.create_tables().unwrap();
            db.insert_named_txs(
                &[NamedTx::new(
                    name.to_owned(),
                    Default::default(),
                    Some(Address::repeat_byte(0xAB)),
                )],
                "http://localhost:8545",
                Default::default(),
            )
            .unwrap();
            db
        }

        #[test]
        fn find_placeholder_uses_labeled_db_key() {
            let db = setup_db_with_named_tx("Token_v2");
            let cfg = TestConfig::default();
            let mut map = HashMap::new();

            // With label "v2", lookup for "{Token}" should query "Token_v2" in the DB
            cfg.find_placeholder_values(
                "{Token}",
                &mut map,
                &db,
                "http://localhost:8545",
                Default::default(),
                Some("v2"),
            )
            .unwrap();

            assert_eq!(map.len(), 1);
            assert!(map.contains_key("Token"));
        }

        #[test]
        fn find_placeholder_without_label_uses_plain_key() {
            let db = setup_db_with_named_tx("Token");
            let cfg = TestConfig::default();
            let mut map = HashMap::new();

            // Without label, lookup for "{Token}" should query "Token" in the DB
            cfg.find_placeholder_values(
                "{Token}",
                &mut map,
                &db,
                "http://localhost:8545",
                Default::default(),
                None,
            )
            .unwrap();

            assert_eq!(map.len(), 1);
            assert!(map.contains_key("Token"));
        }

        #[test]
        fn find_placeholder_with_label_misses_unlabeled_entry() {
            // DB has "Token" (no label), but we look up with label "v2"
            let db = setup_db_with_named_tx("Token");
            let cfg = TestConfig::default();
            let mut map = HashMap::new();

            // Should fail because "Token_v2" doesn't exist in the DB
            let result = cfg.find_placeholder_values(
                "{Token}",
                &mut map,
                &db,
                "http://localhost:8545",
                Default::default(),
                Some("v2"),
            );

            assert!(result.is_err());
        }
    }
}
