use crate::error::ContenderError;
use crate::generator::{
    seeder::{SeedValue, Seeder},
    Generator,
};
use alloy::{
    primitives::{Address, U256},
    rpc::types::TransactionRequest,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::read;

/// Implements `Generator` for TOML files.
/// - `seed` is used to set deterministic sequences of function arguments for the generator
pub struct TestGenerator<'a, T: Seeder> {
    config: TestConfig,
    seed: &'a T,
}

/// TOML file format.
#[derive(Clone, Deserialize, Debug, Serialize)]
pub struct TestConfig {
    /// Setup steps to run before spamming.
    pub setup: Option<Vec<FunctionCallDefinition>>,

    /// Function to call in spam txs.
    pub spam: Option<FunctionCallDefinition>,
}

#[derive(Clone, Deserialize, Debug, Serialize)]
pub struct FunctionCallDefinition {
    /// Address of the contract to call.
    pub to: Address,
    /// Address of the tx sender (must be unlocked on RPC endpoint).
    pub from: Option<Address>,
    /// Name of the function to call.
    pub signature: String,
    /// Parameters to pass to the function.
    pub args: Vec<String>,
    /// Parameters to fuzz during the test.
    pub fuzz: Option<Vec<FuzzParam>>,
}

#[derive(Clone, Deserialize, Debug, Serialize)]
pub struct FuzzParam {
    /// Name of the parameter to fuzz.
    pub name: String,
    /// Minimum value fuzzer will use.
    pub min: Option<U256>,
    /// Maximum value fuzzer will use.
    pub max: Option<U256>,
}

impl<'a, T> TestGenerator<'a, T>
where
    T: Seeder,
{
    pub fn new(config: TestConfig, seed: &'a T) -> Self {
        Self { config, seed }
    }
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

impl<'a, T> Generator for TestGenerator<'a, T>
where
    T: Seeder,
{
    fn get_spam_txs(&self, amount: usize) -> Result<Vec<TransactionRequest>, ContenderError> {
        let mut templates = Vec::new();

        if let Some(function) = &self.config.spam {
            let func = alloy_json_abi::Function::parse(&function.signature).map_err(|e| {
                ContenderError::SpamError("failed to parse function name", Some(e.to_string()))
            })?;

            // hashmap to store fuzzy values
            let mut map: HashMap<String, Vec<U256>> = HashMap::new();

            // pre-generate fuzzy params
            if let Some(fuzz_params) = function.fuzz.as_ref() {
                // NOTE: This will only generate a single 32-byte value for each fuzzed parameter. Fuzzing values in arrays/structs is not yet supported.
                for fparam in fuzz_params.iter() {
                    let values = self
                        .seed
                        .seed_values(amount, fparam.min, fparam.max)
                        .map(|v| v.as_u256())
                        .collect::<Vec<U256>>();
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
                    to: Some(alloy::primitives::TxKind::Call(function.to.clone())),
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
    use crate::generator::RandSeed;
    use std::fs;

    fn get_testconfig() -> TestConfig {
        TestConfig {
            setup: None,
            spam: Some(FunctionCallDefinition {
                to: "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D"
                    .parse::<Address>()
                    .unwrap(),
                from: None,
                signature: "swap(uint256 x, uint256 y, address a, bytes b)".to_string(),
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
            setup: None,
            spam: Some(FunctionCallDefinition {
                to: "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D"
                    .parse::<Address>()
                    .unwrap(),
                from: None,
                signature: "swap(uint256 x, uint256 y, address a, bytes b)".to_string(),
                args: vec![
                    "1".to_owned(),
                    "2".to_owned(),
                    Address::repeat_byte(0x11).to_string(),
                    "0xbeef".to_owned(),
                ],
                fuzz: Some(vec![FuzzParam {
                    name: "x".to_string(),
                    min: None,
                    max: None,
                }]),
            }),
        }
    }

    fn get_setup_testconfig() -> TestConfig {
        TestConfig {
            spam: None,
            setup: Some(vec![
                FunctionCallDefinition {
                    to: "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D"
                        .parse::<Address>()
                        .unwrap(),
                    from: None,
                    signature: "swap(uint256 x, uint256 y, address a, bytes b)".to_string(),
                    args: vec![
                        "1".to_owned(),
                        "2".to_owned(),
                        Address::repeat_byte(0x11).to_string(),
                        "0xdead".to_owned(),
                    ],
                    fuzz: None,
                },
                FunctionCallDefinition {
                    to: "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D"
                        .parse::<Address>()
                        .unwrap(),
                    from: None,
                    signature: "swap(uint256 x, uint256 y, address a, bytes b)".to_string(),
                    args: vec![
                        "1".to_owned(),
                        "2".to_owned(),
                        Address::repeat_byte(0x11).to_string(),
                        "0xbeef".to_owned(),
                    ],
                    fuzz: None,
                },
            ]),
        }
    }

    #[test]
    fn parses_testconfig_toml() {
        let test_file = TestConfig::from_file("univ2ConfigTest.toml").unwrap();
        println!("{:?}", test_file);
        assert_eq!(
            test_file.spam.unwrap().to,
            "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D"
                .parse::<Address>()
                .unwrap()
        )
    }

    fn print_testconfig(cfg: &str) {
        println!("{}", "-".repeat(80));
        println!("{}", cfg);
        println!("{}", "-".repeat(80));
    }

    #[test]
    fn encodes_testconfig_toml() {
        let test_file = get_testconfig();
        let encoded = test_file.encode_toml().unwrap();
        print_testconfig(&encoded);

        let test_file2: TestConfig = get_fuzzy_testconfig();
        let encoded = test_file2.encode_toml().unwrap();
        print_testconfig(&encoded);

        let test_file3: TestConfig = get_setup_testconfig();
        let encoded = test_file3.encode_toml().unwrap();
        print_testconfig(&encoded);

        test_file.save_toml("cargotest.toml").unwrap();
        let test_file2 = TestConfig::from_file("cargotest.toml").unwrap();
        let func = test_file.spam.unwrap();
        assert_eq!(func.to, test_file2.spam.unwrap().to);
        assert_eq!(func.args[0], "1");
        assert_eq!(func.args[1], "2");
        fs::remove_file("cargotest.toml").unwrap();
    }

    #[test]
    fn gets_spam_txs() {
        let test_file = get_testconfig();
        let seed = RandSeed::new();
        let test_gen = TestGenerator::new(test_file, &seed);
        // this seed can be used to recreate the same test tx(s)
        let spam_txs = test_gen.get_spam_txs(1).unwrap();
        println!("generated test tx(s): {:?}", spam_txs);
        assert_eq!(spam_txs.len(), 1);
        let data = spam_txs[0].input.input.to_owned().unwrap().to_string();
        assert_eq!(data, "0x022c0d9f00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000002000000000000000000000000111111111111111111111111111111111111111100000000000000000000000000000000000000000000000000000000000000800000000000000000000000000000000000000000000000000000000000000002dead000000000000000000000000000000000000000000000000000000000000");
    }

    #[test]
    fn fuzz_is_deterministic() {
        let test_file = get_fuzzy_testconfig();
        let seed = RandSeed::from_bytes(&[0x01; 32]);
        let test_gen = TestGenerator::new(test_file, &seed);
        let num_txs = 3;
        let spam_txs_1 = test_gen.get_spam_txs(num_txs).unwrap();
        let spam_txs_2 = test_gen.get_spam_txs(num_txs).unwrap();
        for i in 0..num_txs {
            let data1 = spam_txs_1[i].input.input.to_owned().unwrap().to_string();
            let data2 = spam_txs_2[i].input.input.to_owned().unwrap().to_string();
            assert_eq!(data1, data2);
            println!("data1: {}", data1);
            println!("data2: {}", data2);
        }
    }
}
