mod test_config;

pub use test_config::TestConfig;

#[cfg(test)]
pub mod tests {
    use super::TestConfig;
    use alloy::{
        consensus::TxType,
        hex::ToHexExt,
        node_bindings::{Anvil, AnvilInstance},
        primitives::{Address, U256},
        signers::local::PrivateKeySigner,
    };
    use contender_core::generator::templater::Templater;
    use contender_core::{
        db::MockDb,
        generator::{
            named_txs::ExecutionRequest,
            types::{
                BundleCallDefinition, CreateDefinition, FunctionCallDefinition, FuzzParam,
                PlanType, SpamRequest,
            },
            Generator, RandSeed,
        },
        test_scenario::{TestScenario, TestScenarioParams},
    };
    use std::{collections::HashMap, fs, str::FromStr};
    use tokio::sync::OnceCell;

    // prometheus
    static PROM: OnceCell<prometheus::Registry> = OnceCell::const_new();
    static HIST: OnceCell<prometheus::Histogram> = OnceCell::const_new();

    pub fn spawn_anvil() -> AnvilInstance {
        Anvil::new().block_time(1).try_spawn().unwrap()
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
        let fncall = FunctionCallDefinition {
            to: "0x7a250d5630B4cF539739dF2C5dAcb4c659F248DD".to_owned(),
            from: "0x7a250d5630B4cF539739dF2C5dAcb4c659F248DD"
                .to_owned()
                .into(),
            from_pool: None,
            signature: "swap(uint256 x, uint256 y, address a, bytes b)".to_owned(),
            args: vec![
                "1".to_owned(),
                "2".to_owned(),
                Address::repeat_byte(0x11).encode_hex(),
                "0xdead".to_owned(),
            ]
            .into(),
            fuzz: None,
            value: None,
            kind: None,
            gas_limit: None,
        };

        TestConfig {
            env: None,
            create: None,
            setup: None,
            spam: vec![SpamRequest::Tx(fncall)].into(),
        }
    }

    pub fn get_fuzzy_testconfig() -> TestConfig {
        let fn_call = |data: &str, from_addr: &str| FunctionCallDefinition {
            to: "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D".to_owned(),
            from: from_addr.to_owned().into(),
            from_pool: None,
            value: None,
            signature: "swap(uint256 x, uint256 y, address a, bytes b)".to_owned(),
            args: vec![
                "1".to_owned(),
                "2".to_owned(),
                Address::repeat_byte(0x11).encode_hex(),
                data.to_owned(),
            ]
            .into(),
            kind: None,
            fuzz: vec![FuzzParam {
                param: Some("x".to_string()),
                value: None,
                min: None,
                max: None,
            }]
            .into(),
            gas_limit: None,
        };
        TestConfig {
            env: None,
            create: None,
            setup: None,
            spam: vec![
                SpamRequest::Tx(fn_call(
                    "0xbeef",
                    "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266",
                )),
                SpamRequest::Tx(fn_call(
                    "0xea75",
                    "0x70997970C51812dc3A010C7d01b50e0d17dc79C8",
                )),
                SpamRequest::Tx(fn_call(
                    "0xf00d",
                    "0x3C44CdDdB6a900fa2b585dd299e03d12FA4293BC",
                )),
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
                FunctionCallDefinition {
                    to: "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D".to_owned(),
                    from: "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"
                        .to_owned()
                        .into(),
                    from_pool: None,
                    value: Some("4096".to_owned()),
                    signature: "swap(uint256 x, uint256 y, address a, bytes b)".to_owned(),
                    args: vec![
                        "1".to_owned(),
                        "2".to_owned(),
                        Address::repeat_byte(0x11).encode_hex(),
                        "0xdead".to_owned(),
                    ]
                    .into(),
                    kind: None,
                    fuzz: None,
                    gas_limit: None,
                },
                FunctionCallDefinition {
                    to: "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D".to_owned(),
                    from: "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"
                        .to_owned()
                        .into(),
                    from_pool: None,
                    value: Some("0x1000".to_owned()),
                    signature: "swap(uint256 x, uint256 y, address a, bytes b)".to_owned(),
                    args: vec![
                        "1".to_owned(),
                        "2".to_owned(),
                        Address::repeat_byte(0x11).encode_hex(),
                        "0xbeef".to_owned(),
                    ]
                    .into(),
                    kind: None,
                    fuzz: None,
                    gas_limit: None,
                },
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
                bytecode: COUNTER_BYTECODE.to_string(),
                name: "test_counter".to_string(),
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

    #[test]
    fn parses_testconfig_toml() {
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

    fn print_testconfig(cfg: &str) {
        println!("{}", "-".repeat(80));
        println!("{}", cfg);
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
                agent_store: Default::default(),
                tx_type,
                gas_price_percent_add: None,
                pending_tx_timeout_secs: 12,
            },
            None,
            (&PROM, &HIST),
        )
        .await
        .unwrap();
        // this seed can be used to recreate the same test tx(s)
        let spam_txs = test_gen
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
                agent_store: Default::default(),
                tx_type,
                gas_price_percent_add: None,
                pending_tx_timeout_secs: 12,
            },
            None,
            (&PROM, &HIST),
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
                agent_store: Default::default(),
                tx_type,
                gas_price_percent_add: None,
                pending_tx_timeout_secs: 12,
            },
            None,
            (&PROM, &HIST),
        )
        .await
        .unwrap();

        let num_txs = 13;
        let spam_txs_1 = scenario1
            .load_txs(PlanType::Spam(num_txs, |_| Ok(None)))
            .await
            .unwrap();
        let spam_txs_2 = scenario2
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
            )
            .unwrap();

        assert_eq!(placeholder_map.len(), 3);
    }
}
