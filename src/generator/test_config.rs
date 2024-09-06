use alloy::{
    primitives::{keccak256, Address, U256},
    rpc::types::TransactionRequest,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::read;

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
    pub args: Vec<String>,
    /// Parameters to fuzz during the test.
    pub fuzz: Option<Vec<FuzzParam>>,
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
            let func = alloy_json_abi::Function::parse(&function.name).map_err(|e| {
                ContenderError::SpamError("failed to parse function name", Some(e.to_string()))
            })?;

            // hashmap to store fuzzy values
            let mut map: HashMap<String, Vec<U256>> = HashMap::new();
            // seed for random-looking values
            let seed = seed.unwrap_or_default();

            // pre-generate fuzzy params
            if let Some(fuzz_params) = function.fuzz.as_ref() {
                // NOTE: This will only generate a single 32-byte value for each fuzzed parameter. Fuzzing values in arrays/structs is not yet supported.
                for fparam in fuzz_params.iter() {
                    // TODO: Plug into an externally-defined fuzz generator. This is too restrictive.
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
                    map.insert(fparam.name.to_owned(), values);
                }
            }

            // generate spam txs
            for i in 0..amount {
                // encode function arguments
                let mut args = Vec::new();
                for j in 0..function.args.len() {
                    let maybe_fuzz = || {
                        let input_def = func.inputs[j].to_string();
                        // there's probably a better way to do this, but I haven't found it
                        let arg_namedefs =
                            input_def.split_ascii_whitespace().collect::<Vec<&str>>();
                        if arg_namedefs.len() < 2 {
                            // can't fuzz unnamed params
                            return None;
                        }
                        let arg_name = arg_namedefs[1];
                        if map.contains_key(arg_name) {
                            return Some(map.get(arg_name).unwrap()[i].to_string());
                        }
                        None
                    };

                    let val = maybe_fuzz().unwrap_or_else(|| {
                        // if fuzzing is not enabled, use default value given in config file
                        function.args[j].to_owned()
                    });
                    args.push(val);
                }

                let input =
                    foundry_common::abi::encode_function_args(&func, args).map_err(|e| {
                        ContenderError::SpamError(
                            "failed to encode function arguments.",
                            Some(e.to_string()),
                        )
                    })?;

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
                name: "swap(uint256 x, uint256 y, address a, bytes b)".to_string(),
                args: vec![
                    "1".to_owned(),
                    "2".to_owned(),
                    Address::repeat_byte(0x11).to_string(),
                    "0xdead".to_owned(),
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
                name: "swap(uint256 x, uint256 y, address a, bytes b)".to_string(),
                args: vec![
                    "1".to_owned(),
                    "2".to_owned(),
                    Address::repeat_byte(0x11).to_string(),
                    "0xbeef".to_owned(),
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
        assert_eq!(func.args[0], "1");
        assert_eq!(func.args[1], "2");
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
        assert_eq!(data, "0x022c0d9f00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000002000000000000000000000000111111111111111111111111111111111111111100000000000000000000000000000000000000000000000000000000000000800000000000000000000000000000000000000000000000000000000000000002dead000000000000000000000000000000000000000000000000000000000000");
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
