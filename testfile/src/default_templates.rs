use crate::types::TestConfig;
use alloy::{hex::ToHexExt, primitives::Address};
use contender_core::generator::types::{CreateDefinition, FunctionCallDefinition};

pub struct FillBlockParams {
    pub basepath: String,
    pub from: Address,
    pub gas_target: u64,
}

pub enum DefaultConfig {
    /// Fill the block with spam transactions.
    FillBlock(FillBlockParams),
    // TODO
    // UniswapV2,
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
            DefaultConfig::FillBlock(params) => TestConfig {
                env: None,
                create: Some(vec![CreateDefinition {
                    bytecode: get_bytecode(&params.basepath, "SpamMe"),
                    name: "SpamMe".to_owned(),
                    from: params.from.encode_hex(),
                }]),
                setup: None,
                spam: vec![FunctionCallDefinition {
                    to: "{SpamMe}".to_owned(),
                    from: params.from.encode_hex(),
                    signature: "consumeGas(uint256)".to_owned(),
                    args: vec![params.gas_target.to_string()].into(),
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
