use alloy::{
    dyn_abi::DynSolValue,
    primitives::{keccak256, Address, Bytes, U256},
    rpc::types::TransactionRequest,
};
use serde::{Deserialize, Serialize};
use std::{fs::read, str::FromStr};

use crate::error::ContenderError;

use super::SpamTarget;

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
    pub fn parse_toml(file_path: &str) -> Result<TestConfig, Box<dyn std::error::Error>> {
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

impl SpamTarget for TestConfig {
    fn get_spam_txs(&self) -> Result<Vec<TransactionRequest>, ContenderError> {
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
            println!("encoding calldata: {}", fn_sig);
            let sig_hash = keccak256(fn_sig);
            let fn_selector = sig_hash.split_at(4).0;

            // encode function arguments
            let mut args = Vec::new();
            for arg in function.params.iter() {
                let val = match arg.r#type.as_str() {
                    "uint256" => {
                        let val = U256::from_str(&arg.value)
                            .map_err(|_e| ContenderError::SpamError("failed to parse uint256"))?;
                        DynSolValue::Uint(val, 256)
                    }
                    "address" => {
                        let val = Address::from_str(&arg.value)
                            .map_err(|_e| ContenderError::SpamError("failed to parse address"))?;
                        DynSolValue::Address(val)
                    }
                    "bytes" => {
                        let val = Bytes::from_str(&arg.value)
                            .map_err(|_e| ContenderError::SpamError("failed to parse bytes"))?;
                        DynSolValue::Bytes(val.to_vec())
                    }
                    _ => return Err(ContenderError::SpamError("unsupported type")),
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

    #[test]
    fn test_parse_testconfig_toml() {
        let test_file = TestConfig::parse_toml("univ2ConfigTest.toml").unwrap();
        println!("{:?}", test_file);
        assert_eq!(
            test_file.to,
            "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D"
                .parse::<Address>()
                .unwrap()
        )
    }

    #[test]
    fn test_encode_testconfig_toml() {
        let test_file = get_testconfig();
        let encoded = test_file.encode_toml().unwrap();
        println!("{}", encoded);

        test_file.save_toml("cargotest.toml").unwrap();
        let test_file2 = TestConfig::parse_toml("cargotest.toml").unwrap();
        assert_eq!(test_file.to, test_file2.to);
        let func = test_file.function.unwrap();
        assert_eq!(func.params[0].name, "amount0Out");
        assert_eq!(func.params[1].name, "amount1Out");
        fs::remove_file("cargotest.toml").unwrap();
    }

    #[test]
    fn test_get_spam_txs() {
        let test_file = get_testconfig();
        let spam_txs = test_file.get_spam_txs().unwrap();
        println!("generated test tx(s): {:?}", spam_txs);
        assert_eq!(spam_txs.len(), 1);
        let data = spam_txs[0].input.input.to_owned().unwrap().to_string();
        assert_eq!(data, "0x022c0d9f00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000111111111111111111111111111111111111111100000000000000000000000000000000000000000000000000000000000000800000000000000000000000000000000000000000000000000000000000000000");
    }
}
