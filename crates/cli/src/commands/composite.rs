use std::{error::Error, fs, future::Future, io::Read, str::FromStr, vec};

use alloy::rpc;
use contender_core::generator::RandSeed;
use futures::future::join_all;
use tracing::info;
use tracing_subscriber::fmt::format;
use yaml_rust2::{Yaml, YamlLoader};

use crate::{commands::common::cli_env_vars_parser, util::EngineParams};

use super::{common::AuthCliArgs, setup, SetupCliArgs, SetupCommandArgs};

#[derive(Debug, clap::Args)]
pub struct CompositeScenarioArgs {
    pub filename: Option<String>,
}

pub async fn composite(
    db: &(impl contender_core::db::DbOps + Clone + Send + Sync + 'static),
    args: CompositeScenarioArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    // Read testfile

    let compose_file_name = match args.filename {
        Some(filepath) => filepath,
        None => String::from("./contender-compose.yml"),
    };

    println!("Compose file {:?}", &compose_file_name);

    let scenarios = get_setup_from_compose_file(compose_file_name)?;

    for scenario in scenarios {
        info!("================================================================================================= Setting up contract for: {} =================================================================================================", scenario.name);
        setup(db, scenario.config).await?;
    }

    Ok(())
}

pub struct ComposeFileScenario {
    pub name: String,
    pub config: SetupCommandArgs,
}

fn get_setup_from_compose_file(
    compose_file_path: String,
) -> Result<Vec<ComposeFileScenario>, Box<dyn Error>> {
    // Read file
    let file_contents = fs::read(&compose_file_path)
        .map_err(|_e| format!("Can't read the file on path {}", &compose_file_path))?;

    let yaml_file_contents = String::from_utf8_lossy(&file_contents).to_string();

    let compose_file_contents =
        YamlLoader::load_from_str(&yaml_file_contents).map_err(|_e| "Yaml File failed to parse")?;

    if compose_file_contents.is_empty() {
        return Err("Compose file is empty".into());
    }
    let contracts_list = &compose_file_contents[0]["setup"];

    let mut setup_scenarios = vec![];

    let contracts_hash = contracts_list
        .as_hash()
        .ok_or_else(|| "Setup section is not an object".to_string())?;

    for (scenario_name, setup_command_params) in contracts_hash.into_iter() {
        let scenario_object = setup_command_params
            .clone()
            .into_hash()
            .ok_or(format!("Malformed scenario {:?}", setup_command_params))?;

        let testfile = String::from(
            setup_command_params["testfile"]
                .as_str()
                .ok_or_else(|| "Missing or invalid value for 'testfile'".to_string())?,
        );

        let rpc_url = match scenario_object.get(&Yaml::String("rpc_url".into())) {
            Some(rpc_url_value) => {
                if let Some(url) = rpc_url_value.as_str() {
                    String::from(url)
                } else {
                    return Err("Invalid type value for 'rpc_url'".into());
                }
            }
            None => String::from("http://localhost:8545"),
        };

        let min_balance = match scenario_object.get(&Yaml::String("min_balance".into())) {
            Some(min_balance_value) => {
                // check proper type
                match min_balance_value {
                    Yaml::Real(float_str) => Ok(float_str.clone()),
                    Yaml::Integer(int_val) => Ok(int_val.to_string()),
                    Yaml::String(str_val) => {
                        // Try to parse the string as a number to validate it
                        if str_val.parse::<f64>().is_ok() {
                            Ok(str_val.clone())
                        } else {
                            Err(format!("Invalid min_balance string value: {:?}", str_val))
                        }
                    }
                    _ => Err(format!("Invalid min_balance type: {:?}", min_balance_value)),
                }
            }
            None => Ok("0.01".to_string()),
        }?;

        let env_variables = match scenario_object.get(&Yaml::String("env".into())) {
            Some(value) => {
                match value {
                    Yaml::Array(env_vars) => {
                        let mut temp_env_variables = vec![];
                        for item in env_vars {
                            if let Some(env_value_pair) = item.as_str() {
                                temp_env_variables.push(cli_env_vars_parser(env_value_pair)?);
                            }
                        }
                        Ok(Some(temp_env_variables))
                    },
                    _ => Err(format!("Invalid env_value type: {:?}", &value)),
                }
            },
            None => Ok(None)
        }?;

        let private_keys = match scenario_object.get(&Yaml::String("private_keys".into())) {
            Some(value) => {
                match value {
                    Yaml::Array(priv_keys) => {
                        let mut temp_private_keys = vec![];
                        for item in priv_keys {
                            if let Some(p_key) = item.as_str() {
                                temp_private_keys.push(p_key.to_string());
                            }
                        }
                        Ok(Some(temp_private_keys))
                    },
                    _ => Err(format!("Invalid env_value type: {:?}", &value)),
                }
            },
            None => Ok(None)
        }?;


        let tx_type = match scenario_object.get(&Yaml::String("tx_type".into())) {
            Some(value) => {
                match value {
                    Yaml::String(tx_type_string) => {
                        match tx_type_string.as_str() {
                            "legacy" => Ok(alloy::consensus::TxType::Legacy),
                            "eip1559" => Ok(alloy::consensus::TxType::Eip1559),
                            _ => Err(format!("Invalid Value for 'tx_type' = {}", tx_type_string.as_str()))
                        }
                    },
                    _ => Err(format!("Invalid type value for 'tx_type' = {:?}", value))
                }
            },
            None => Ok(alloy::consensus::TxType::Eip1559 )
        }?;

        setup_scenarios.push(ComposeFileScenario {
            name: scenario_name.as_str().unwrap().to_owned(),
            config: SetupCommandArgs {
                testfile,
                rpc_url,
                min_balance,
                env: env_variables,
                tx_type,
                private_keys,

                // TODO: Hardcoded parameters for now, need more understanding on where to get these from
                seed: RandSeed::new(),
                engine_params: EngineParams {
                    engine_provider: None,
                    call_fcu: false,
                },
            },
        });
    }
    Ok(setup_scenarios)
}

#[cfg(test)]
mod test {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_temp_yaml_file(content: &str) -> Result<NamedTempFile, Box<dyn Error>> {
        let mut temp_file = NamedTempFile::new()?;
        temp_file.write_all(content.as_bytes())?;
        temp_file.flush()?;
        Ok(temp_file)
    }

    #[test]
    fn non_existent_compose_file() -> Result<(), Box<dyn Error>> {
        let scenarios = get_setup_from_compose_file("./non-existent-scene.yml".to_string());
        assert_eq!(
            scenarios.err().unwrap().to_string(),
            "Can't read the file on path ./non-existent-scene.yml"
        );
        Ok(())
    }

    // Done
    #[test]
    fn test_basic_valid_yaml() -> Result<(), Box<dyn Error>> {
        let yaml_content = r#"
setup:
  scenario1:
    testfile: "./tests/test1.js"
    rpc_url: "http://localhost:9545"
    min_balance: '5'
    tx_type: "eip1559"
    private_keys:
      - "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef"
      - "0xabcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890"
    env:
      - "KEY1=value1"
      - "KEY2=value2"
"#;
        let temp_file = create_temp_yaml_file(yaml_content)?;
        let scenarios =
            get_setup_from_compose_file(temp_file.path().to_string_lossy().to_string())?;

        assert_eq!(scenarios.len(), 1);
        assert_eq!(scenarios[0].name, "scenario1");
        assert_eq!(scenarios[0].config.testfile, "./tests/test1.js");
        assert_eq!(scenarios[0].config.rpc_url, "http://localhost:9545");
        assert_eq!(scenarios[0].config.min_balance, "5");
        assert_eq!(
            scenarios[0].config.tx_type,
            alloy::consensus::TxType::Eip1559
        );

        let private_keys = scenarios[0].config.private_keys.as_ref().unwrap();
        assert_eq!(private_keys.len(), 2);
        assert_eq!(
            private_keys[0],
            "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef"
        );

        let env_vars = scenarios[0].config.env.as_ref().unwrap();
        assert_eq!(env_vars.len(), 2);
        // Assuming cli_env_vars_parser returns a tuple of (key, value)
        assert_eq!(env_vars[0].0, "KEY1");
        assert_eq!(env_vars[0].1, "value1");

        Ok(())
    }

    // Done
    #[test]
    fn test_missing_testfile() {
        let yaml_content = r#"
setup:
  missing_testfile:
    rpc_url: "http://localhost:8545"
"#;
        let temp_file = create_temp_yaml_file(yaml_content).unwrap();
        let result = get_setup_from_compose_file(temp_file.path().to_string_lossy().to_string());

        assert!(result.is_err());
        match result {
            Err(e) => assert!(e
                .to_string()
                .contains("Missing or invalid value for 'testfile'")),
            _ => panic!("Expected an error for missing testfile"),
        }
    }

    // Done
    #[test]
    fn test_default_values() -> Result<(), Box<dyn Error>> {
        let yaml_content = r#"
setup:
  scenario1:
    testfile: "./tests/test1.js"
"#;
        let temp_file = create_temp_yaml_file(yaml_content)?;
        let scenarios =
            get_setup_from_compose_file(temp_file.path().to_string_lossy().to_string())?;

        assert_eq!(scenarios.len(), 1);
        assert_eq!(scenarios[0].name, "scenario1");
        assert_eq!(scenarios[0].config.testfile, "./tests/test1.js");
        assert_eq!(scenarios[0].config.rpc_url, "http://localhost:8545");
        assert_eq!(scenarios[0].config.min_balance, "0.01"); // Default value
        assert_eq!(scenarios[0].config.tx_type, alloy::consensus::TxType::Eip1559);
        assert_eq!(scenarios[0].config.env, None);
        assert_eq!(scenarios[0].config.private_keys, None);
        Ok(())
    }

    // Done
    #[test]
    fn test_legacy_tx_type() -> Result<(), Box<dyn Error>> {
        let yaml_content = r#"
setup:
  legacy_scenario:
    testfile: "./tests/legacy.js"
    tx_type: "legacy"
"#;
        let temp_file = create_temp_yaml_file(yaml_content)?;
        let scenarios =
            get_setup_from_compose_file(temp_file.path().to_string_lossy().to_string())?;

        assert_eq!(scenarios.len(), 1);
        assert_eq!(
            scenarios[0].config.tx_type,
            alloy::consensus::TxType::Legacy
        );

        Ok(())
    }

        // Done
    #[test]
    fn test_eip1559_tx_type() -> Result<(), Box<dyn Error>> {
        let yaml_content = r#"
setup:
  legacy_scenario:
    testfile: "./tests/legacy.js"
    tx_type: eip1559
"#;
        let temp_file = create_temp_yaml_file(yaml_content)?;
        let scenarios =
            get_setup_from_compose_file(temp_file.path().to_string_lossy().to_string())?;

        assert_eq!(
            scenarios[0].config.tx_type,
            alloy::consensus::TxType::Eip1559
        );

        Ok(())
    }

    // Done
    #[test]
    fn test_invalid_tx_type() {
        let yaml_content = r#"
setup:
  invalid_scenario:
    testfile: "./tests/invalid.js"
    tx_type: "invalid_type"
"#;
        let temp_file = create_temp_yaml_file(yaml_content).unwrap();
        let result = get_setup_from_compose_file(temp_file.path().to_string_lossy().to_string());

        assert!(result.is_err());
        assert_eq!(result.err().unwrap().to_string(), "Invalid Value for 'tx_type' = invalid_type".to_string())
    }

    // Done
    #[test]
    fn test_valid_env_variables() -> Result<(), Box<dyn std::error::Error>>{
                let yaml_content = r#"
setup:
  invalid_scenario:
    testfile: "./tests/some.toml"
    env:
        - KEY1=VALUE1
        - KEY2=VALUE2
"#;
        let temp_file = create_temp_yaml_file(yaml_content).unwrap();
        let result = get_setup_from_compose_file(temp_file.path().to_string_lossy().to_string())?;

        assert_eq!(result[0].config.env.as_ref().unwrap()[0].0.to_string(), "KEY1".to_string());
        assert_eq!(result[0].config.env.as_ref().unwrap()[0].1, "VALUE1".to_string());
        assert_eq!(result[0].config.env.as_ref().unwrap()[1].0, "KEY2".to_string());
        assert_eq!(result[0].config.env.as_ref().unwrap()[1].1, "VALUE2".to_string());
        
        Ok(())
    }



    #[test]
    fn test_non_existent_file() -> Result<(), Box<dyn Error>> {
        let result = get_setup_from_compose_file("non_existent_file.yaml".to_string());
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn test_malformed_yaml() -> Result<(), Box<dyn Error>> {
        let yaml_content = r#"
setup:
  malformed:
    testfile: "test.js"
    min_balance: ]
"#;
        let temp_file = create_temp_yaml_file(yaml_content).unwrap();
        let result = get_setup_from_compose_file(temp_file.path().to_string_lossy().to_string());

        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn test_empty_yaml() {
        let yaml_content = "";
        let temp_file = create_temp_yaml_file(yaml_content).unwrap();
        let result = get_setup_from_compose_file(temp_file.path().to_string_lossy().to_string());

        assert!(result.is_err());
    }

    #[test]
    fn test_numeric_values_for_min_balance() -> Result<(), Box<dyn Error>> {
        let yaml_content = r#"
setup:
  numeric_scenario:
    testfile: "./tests/numeric.js"
    min_balance: "5"
"#;
        let temp_file = create_temp_yaml_file(yaml_content)?;
        let scenarios =
            get_setup_from_compose_file(temp_file.path().to_string_lossy().to_string())?;

        assert_eq!(scenarios[0].config.min_balance, "5");

        Ok(())
    }

    #[test]
    fn test_empty_private_keys_and_envs() -> Result<(), Box<dyn Error>> {
        let yaml_content = r#"
setup:
  empty_arrays:
    testfile: "./tests/empty.js"
    private_keys: []
    env: []
"#;
        let temp_file = create_temp_yaml_file(yaml_content)?;
        let scenarios =
            get_setup_from_compose_file(temp_file.path().to_string_lossy().to_string())?;

        let private_keys = scenarios[0].config.private_keys.as_ref().unwrap();
        assert_eq!(private_keys.len(), 0);

        let env_vars = scenarios[0].config.env.as_ref().unwrap();
        assert_eq!(env_vars.len(), 0);

        Ok(())
    }

    #[test]
    fn test_no_setup_section() {
        let yaml_content = r#"
version: '3'
services:
  web:
    image: nginx
"#;
        let temp_file = create_temp_yaml_file(yaml_content).unwrap();
        let result = get_setup_from_compose_file(temp_file.path().to_string_lossy().to_string());

        assert!(result.is_err() || result.unwrap().is_empty());
    }
}
