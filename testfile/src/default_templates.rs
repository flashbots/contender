use crate::types::TestConfig;
use alloy::{hex::ToHexExt, primitives::Address};
use contender_core::generator::types::{CreateDefinition, FunctionCallDefinition};

#[derive(Clone)]
pub enum DefaultConfig {
    /// Fill the block with spam transactions.
    FillBlock(String, Address),
    // TODO
    // UniswapV2,
}

impl From<(String, String)> for DefaultConfig {
    fn from((basepath, name): (String, String)) -> Self {
        match name.to_lowercase().as_str() {
            "fillblock" => DefaultConfig::FillBlock(
                basepath,
                "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"
                    .parse()
                    .unwrap(),
            ),
            _ => panic!("Unknown default config: {}", name),
        }
    }
}

fn get_bytecode(artifacts_path: &str, name: &str) -> String {
    let artifact_path = format!("{artifacts_path}/{name}.sol/{name}.json");
    let artifact = std::fs::read_to_string(artifact_path).unwrap();
    let artifact: serde_json::Value = serde_json::from_str(&artifact).unwrap();
    let bytecode = artifact["bytecode"]["object"].as_str().unwrap();
    bytecode.to_owned()
}

impl From<DefaultConfig> for TestConfig {
    fn from(config: DefaultConfig) -> Self {
        match config {
            DefaultConfig::FillBlock(basepath, from) => TestConfig {
                env: None,
                create: Some(vec![CreateDefinition {
                    bytecode: get_bytecode(&basepath, "SpamMe"),
                    name: "SpamMe".to_owned(),
                    from: from.encode_hex(),
                }]),
                setup: None,
                spam: vec![FunctionCallDefinition {
                    to: "{SpamMe}".to_owned(),
                    from: from.encode_hex(),
                    signature: "consumeGas(uint256)".to_owned(),
                    args: vec!["30000000".to_owned()].into(),
                    value: None,
                    fuzz: None,
                }]
                .into(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_bytecode() {
        let bytecode = get_bytecode("./contracts/out", "SpamMe");
        assert_eq!(bytecode.len(), 2600);
        assert!(bytecode.starts_with("0x6080"));
    }
}
