use alloy::{
    primitives::{Address, Bytes, U256},
    rpc::types::TransactionRequest,
};
use serde::{Deserialize, Serialize};
use std::fs::read;

use crate::error::ContenderError;

use super::SpamTarget;

#[derive(Deserialize, Debug, Serialize)]
pub struct TestFile {
    pub to: Address,
    pub from: Option<Address>,
    pub function: Option<FunctionDefinition>,
}

#[derive(Deserialize, Debug, Serialize)]
pub struct FunctionDefinition {
    pub name: String,
    pub params: Vec<Param>,
    pub fuzz: Option<Vec<FuzzParam>>,
}

#[derive(Deserialize, Debug, Serialize)]
pub struct Param {
    pub name: String,
    pub r#type: String,
    /// Default value; may be overridden by including the param in the fuzz section of the config file.
    pub value: String,
}

#[derive(Deserialize, Debug, Serialize)]
pub struct FuzzParam {
    pub name: String,
    pub min: Option<U256>,
    pub max: Option<U256>,
}

impl TestFile {
    pub fn parse(file_path: &str) -> Result<TestFile, Box<dyn std::error::Error>> {
        let file_contents = read(file_path)?;
        let file_contents_str = String::from_utf8_lossy(&file_contents).to_string();
        let test_file: TestFile = toml::from_str(&file_contents_str)?;
        Ok(test_file)
    }

    pub fn encode(&self) -> Result<String, Box<dyn std::error::Error>> {
        let encoded = toml::to_string(self)?;
        Ok(encoded)
    }

    pub fn save(&self, file_path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let encoded = self.encode()?;
        std::fs::write(file_path, encoded)?;
        Ok(())
    }
}

impl SpamTarget for TestFile {
    fn get_spam_txs(&self) -> Result<Vec<TransactionRequest>, ContenderError> {
        let mut templates = Vec::new();

        if let Some(function) = &self.function {
            let input = Bytes::default(); // TODO: abi-encode data from `function` into calldata
            let tx = alloy::rpc::types::TransactionRequest {
                to: Some(alloy::primitives::TxKind::Call(self.to.clone())),
                input: alloy::rpc::types::TransactionInput::both(input),
                ..Default::default()
            };
            templates.push(tx);
        }

        Ok(templates)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn test_parse_test_file() {
        let test_file = TestFile::parse("univ2ConfigTest.toml").unwrap();
        println!("{:?}", test_file);
        assert_eq!(
            test_file.to,
            "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D"
                .parse::<Address>()
                .unwrap()
        )
    }

    #[test]
    fn test_encode_test_file() {
        let test_file = TestFile {
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
                        value: Address::repeat_byte(0x01).to_string(),
                    },
                    Param {
                        name: "data".to_string(),
                        r#type: "bytes".to_string(),
                        value: "0x".to_string(),
                    },
                ],
                fuzz: None,
            }),
        };
        let encoded = test_file.encode().unwrap();
        println!("{}", encoded);

        test_file.save("cargotest.toml").unwrap();
        let test_file2 = TestFile::parse("cargotest.toml").unwrap();
        assert_eq!(test_file.to, test_file2.to);
        let func = test_file.function.unwrap();
        assert_eq!(func.params[0].name, "amount0Out");
        assert_eq!(func.params[1].name, "amount1Out");
        fs::remove_file("cargotest.toml").unwrap();
    }
}
