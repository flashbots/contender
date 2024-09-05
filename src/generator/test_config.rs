use alloy::{
    dyn_abi::DynSolValue,
    primitives::{keccak256, Address, Bytes, U256},
    rpc::types::TransactionRequest,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::{fs::read, str::FromStr};

use crate::error::ContenderError;

use super::{rand_seed::RandSeed, Generator};

/// Testfile
#[derive(Deserialize, Debug, Serialize)]
pub struct TestConfig {
    /// Address of the contract to call.
    pub to: Address,
    /// Address of the account to call the contract from (must be unlocked on RPC endpoint).
    pub from: Option<Address>,
    /// Function-specific test configuration.
    pub function: Option<FunctionDefinition>,
}

#[derive(Deserialize, Debug, Serialize)]
pub struct FunctionDefinition {
    /// Name of the function to call.
    pub name: String,
    /// Parameters to pass to the function.
    pub params: Vec<Param>,
    /// Parameters to fuzz during the test.
    pub fuzz: Option<Vec<FuzzParam>>,
}

#[derive(Deserialize, Debug, Serialize)]
pub struct Param {
    /// Name of the parameter.
    pub name: String,
    /// Solidity type of the parameter.
    pub r#type: String,
    /// Default value; may be overridden by naming the param in the `fuzz`` section of the config file.
    pub value: String,
}

#[derive(Deserialize, Debug, Serialize)]
pub struct FuzzParam {
    /// Name of the parameter to fuzz.
    pub name: String,
    /// Minimum value fuzzer will use.
    pub min: Option<U256>,
    /// Maximum value fuzzer will use.
    pub max: Option<U256>,
}

impl TestConfig {
    pub fn from_file(file_path: &str) -> Result<TestConfig, Box<dyn std::error::Error>> {
        let file_contents = read(file_path)?;
        let file_contents_str = String::from_utf8_lossy(&file_contents).to_string();
        let test_file: TestConfig = toml::from_str(&file_contents_str)?;
        Ok(test_file)
    }

    pub fn encode_toml(&self) -> Result<String, Box<dyn std::error::Error>> {
        let encoded = toml::to_string(self)?;
        Ok(encoded)
    }

    pub fn save_toml(&self, file_path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let encoded = self.encode_toml()?;
        std::fs::write(file_path, encoded)?;
        Ok(())
    }
}

impl Generator for TestConfig {
    fn get_spam_txs(
        &self,
        amount: usize,
        seed: Option<RandSeed>,
    ) -> Result<Vec<TransactionRequest>, ContenderError> {
        let mut templates = Vec::new();

        if let Some(function) = &self.function {
            // encode function selector
            let fn_sig = format!(
                "{}({})",
                function.name,
                function
                    .params
                    .iter()
                    .map(|p| p.r#type.to_owned())
                    .collect::<Vec<_>>()
                    .join(",")
            );
            let sig_hash = keccak256(fn_sig);
            let fn_selector = sig_hash.split_at(4).0;

            // hashmap to store fuzzy values
            let mut map: HashMap<String, Vec<U256>> = HashMap::new();
            // seed for random-looking values
            let seed = seed.unwrap_or_default();

            // pre-generate fuzzy params
            if let Some(fuz) = function.fuzz.as_ref() {
                for fparam in fuz.iter() {
                    let values = (0..amount)
                        .map(|i| {
                            // generate random-looking value between min and max from seed
                            let min = fparam.min.unwrap_or(U256::ZERO);
                            let max = fparam.max.unwrap_or(U256::MAX);
                            let seed_num = seed.as_u256() + U256::from(i);
                            let val = keccak256(seed_num.as_le_slice());
                            let val = U256::from_le_bytes(val.0);
                            let val = val % (max - min) + min;
                            val
                        })
                        .collect();
                    map.insert(fparam.name.clone(), values);
                }
            }

            // generate spam txs
            for i in 0..amount {
                // encode function arguments
                let mut args = Vec::new();
                for arg in function.params.iter() {
                    let maybe_fuzz = || {
                        if map.contains_key(&arg.name) {
                            Some(map.get(&arg.name).unwrap()[i])
                        } else {
                            None
                        }
                    };
                    let val = match arg.r#type.as_str() {
                        // TODO: add remaining types from DynSolValue
                        // TODO: find a general solution for structs/tuples/arrays
                        "uint256" => {
                            let val = if let Some(fuzz_val) = maybe_fuzz() {
                                fuzz_val
                            } else {
                                U256::from_str(&arg.value).map_err(|e| {
                                    ContenderError::SpamError(
                                        "failed to parse uint256",
                                        Some(format!("{:?}", e)),
                                    )
                                })?
                            };
                            DynSolValue::Uint(val, 256)
                        }
                        "address" => {
                            let val = if let Some(fuzz_val) = maybe_fuzz() {
                                Address::from_slice(&fuzz_val.as_le_slice()[0..20])
                            } else {
                                Address::from_str(&arg.value).map_err(|e| {
                                    ContenderError::SpamError(
                                        "failed to parse address",
                                        Some(format!("{:?}", e)),
                                    )
                                })?
                            };
                            DynSolValue::Address(val)
                        }
                        "bytes" => {
                            let val = if let Some(fuzz_val) = maybe_fuzz() {
                                fuzz_val.to_le_bytes::<32>().into()
                            } else {
                                Bytes::from_str(&arg.value).map_err(|e| {
                                    ContenderError::SpamError(
                                        "failed to parse bytes",
                                        Some(format!("{:?}", e)),
                                    )
                                })?
                            };
                            DynSolValue::Bytes(val.to_vec())
                        }
                        "address[]" => {
                            // temporary measure to get the uniswap example working
                            // TODO: delete this branch; delete all branches; this is cursed!
                            let val = if let Some(fuzz_val) = maybe_fuzz() {
                                let mut addresses = Vec::new();
                                for _ in 0..3 {
                                    addresses.push(DynSolValue::Address(Address::from_slice(
                                        &fuzz_val.as_le_slice()[0..20],
                                    )));
                                }
                                addresses
                            } else {
                                Vec::new()
                            };
                            DynSolValue::Array(val)
                        }
                        _ => {
                            // TODO: handle dynamic types here (?)
                            return Err(ContenderError::SpamError(
                                "unsupported type",
                                Some(arg.r#type.to_owned()),
                            ));
                        }
                    };
                    args.push(val);
                }

                // encode function call
                let calldata = DynSolValue::Tuple(args).abi_encode_params();
                let input = [fn_selector, &calldata].concat();

                let tx = alloy::rpc::types::TransactionRequest {
                    to: Some(alloy::primitives::TxKind::Call(self.to.clone())),
                    input: alloy::rpc::types::TransactionInput::both(input.into()),
                    ..Default::default()
                };
                templates.push(tx);
            }
        }

        Ok(templates)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn get_testconfig() -> TestConfig {
        TestConfig {
            to: "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D"
                .parse::<Address>()
                .unwrap(),
            from: None,
            function: Some(FunctionDefinition {
                name: "swap".to_string(),
                params: vec![
                    Param {
                        name: "amount0Out".to_string(),
                        r#type: "uint256".to_string(),
                        value: "0".to_string(),
                    },
                    Param {
                        name: "amount1Out".to_string(),
                        r#type: "uint256".to_string(),
                        value: "0".to_string(),
                    },
                    Param {
                        name: "to".to_string(),
                        r#type: "address".to_string(),
                        value: Address::repeat_byte(0x11).to_string(),
                    },
                    Param {
                        name: "data".to_string(),
                        r#type: "bytes".to_string(),
                        value: "0x".to_string(),
                    },
                ],
                fuzz: None,
            }),
        }
    }

    fn get_fuzzy_testconfig() -> TestConfig {
        TestConfig {
            to: "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D"
                .parse::<Address>()
                .unwrap(),
            from: None,
            function: Some(FunctionDefinition {
                name: "swap".to_string(),
                params: vec![
                    Param {
                        name: "amount0Out".to_string(),
                        r#type: "uint256".to_string(),
                        value: "0".to_string(),
                    },
                    Param {
                        name: "amount1Out".to_string(),
                        r#type: "uint256".to_string(),
                        value: "0".to_string(),
                    },
                    Param {
                        name: "to".to_string(),
                        r#type: "address".to_string(),
                        value: Address::repeat_byte(0x11).to_string(),
                    },
                    Param {
                        name: "data".to_string(),
                        r#type: "bytes".to_string(),
                        value: "0x".to_string(),
                    },
                ],
                fuzz: Some(vec![FuzzParam {
                    name: "data".to_string(),
                    min: None,
                    max: None,
                }]),
            }),
        }
    }

    #[test]
    fn parses_testconfig_toml() {
        let test_file = TestConfig::from_file("univ2ConfigTest.toml").unwrap();
        println!("{:?}", test_file);
        assert_eq!(
            test_file.to,
            "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D"
                .parse::<Address>()
                .unwrap()
        )
    }

    #[test]
    fn encodes_testconfig_toml() {
        let test_file = get_testconfig();
        let encoded = test_file.encode_toml().unwrap();
        println!("{}", encoded);

        test_file.save_toml("cargotest.toml").unwrap();
        let test_file2 = TestConfig::from_file("cargotest.toml").unwrap();
        assert_eq!(test_file.to, test_file2.to);
        let func = test_file.function.unwrap();
        assert_eq!(func.params[0].name, "amount0Out");
        assert_eq!(func.params[1].name, "amount1Out");
        fs::remove_file("cargotest.toml").unwrap();
    }

    #[test]
    fn gets_spam_txs() {
        let test_file = get_testconfig();
        // this seed can be used to recreate the same test tx(s)
        let seed = RandSeed::new();
        let spam_txs = test_file.get_spam_txs(1, Some(seed)).unwrap();
        println!("generated test tx(s): {:?}", spam_txs);
        assert_eq!(spam_txs.len(), 1);
        let data = spam_txs[0].input.input.to_owned().unwrap().to_string();
        assert_eq!(data, "0x022c0d9f00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000111111111111111111111111111111111111111100000000000000000000000000000000000000000000000000000000000000800000000000000000000000000000000000000000000000000000000000000000");
    }

    #[test]
    fn fuzz_is_deterministic() {
        let test_file = get_fuzzy_testconfig();
        let seed = RandSeed::from_bytes(&[0x01; 32]);
        let num_txs = 3;
        let spam_txs_1 = test_file
            .get_spam_txs(num_txs, Some(seed.to_owned()))
            .unwrap();
        let spam_txs_2 = test_file.get_spam_txs(num_txs, Some(seed)).unwrap();
        for i in 0..num_txs {
            let data1 = spam_txs_1[i].input.input.to_owned().unwrap().to_string();
            let data2 = spam_txs_2[i].input.input.to_owned().unwrap().to_string();
            assert_eq!(data1, data2);
            println!("data1: {}", data1);
            println!("data2: {}", data2);
        }
    }
}
