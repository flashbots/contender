use crate::forge_build::run_forge_build;
use crate::types::TestConfig;
use alloy::{hex::ToHexExt, primitives::Address};
use contender_core::error::ContenderError;
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

fn get_bytecode(artifacts_path: &str, name: &str) -> Result<String, Box<dyn std::error::Error>> {
    let artifact_path = format!("{artifacts_path}/{name}.sol/{name}.json");
    let artifact = std::fs::read_to_string(&artifact_path)?;
    let artifact: serde_json::Value = serde_json::from_str(&artifact)?;
    let bytecode = artifact["bytecode"]["object"]
        .as_str()
        .ok_or(ContenderError::SetupError(
            "invalid artifact JSON",
            Some(artifact_path),
        ))?;
    Ok(bytecode.to_owned())
}

impl From<DefaultConfig> for TestConfig {
    fn from(config: DefaultConfig) -> Self {
        match config {
            DefaultConfig::FillBlock(params) => {
                let bytecode = get_bytecode(&params.basepath, "SpamMe");
                let bytecode = if bytecode.is_err() {
                    let trimmed_path = params.basepath.split('/').collect::<Vec<_>>();
                    let trimmed_path: String = trimmed_path
                        .split_at(trimmed_path.len() - 1)
                        .0
                        .into_iter()
                        .map(|s| s.to_string())
                        .reduce(|acc, cur| format!("{}/{}", acc, cur))
                        .expect("invalid base path");
                    println!("running 'forge build' in {}", &trimmed_path);
                    run_forge_build(&trimmed_path).expect("forge is required to build contracts");
                    get_bytecode(&params.basepath, "SpamMe")
                        .expect("forge didn't build the contracts")
                } else {
                    bytecode.expect("this should never happen")
                };
                TestConfig {
                    env: None,
                    create: Some(vec![CreateDefinition {
                        bytecode,
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
                        kind: None,
                    }]
                    .into(),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_bytecode() {
        run_forge_build("./contracts").unwrap();
        let bytecode = get_bytecode("./contracts/out", "SpamMe").unwrap();
        assert_eq!(bytecode.len(), 2600);
        assert!(bytecode.starts_with("0x6080"));
    }
}
