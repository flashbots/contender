use contender_core::generator::RandSeed;
use hashlink::LinkedHashMap;
use yaml_rust2::{Yaml, YamlLoader};

use crate::util::EngineParams;

use super::{common::cli_env_vars_parser, spam, SetupCommandArgs, SpamCommandArgs};
use std::{error::Error, fs, str::FromStr};


#[derive(Clone)]
pub struct ComposeFileScenario {
    pub name: String,
    pub config: SetupCommandArgs,
}

pub struct CompositeSpamConfiguration {
    pub stage_name: String,
    pub spam_configs: Vec<SpamCommandArgs>,
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
            .ok_or(format!("Malformed scenario {setup_command_params:?}"))?;

        let testfile = get_testfile(&scenario_object)?;

        let rpc_url = get_rpc_url(&scenario_object)?;

        let min_balance = get_min_balance(&scenario_object)?;

        let private_keys = get_private_keys(&scenario_object)?;

        let env_variables = get_env_variables(&scenario_object)?;

        let tx_type = get_tx_type(&scenario_object)?;

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


fn get_spam_configuration_from_compose_file(
    compose_file_path: String,
) -> Result<Vec<CompositeSpamConfiguration>, Box<dyn Error>> {
    let file_contents = fs::read(&compose_file_path)
        .map_err(|_e| format!("Can't read the file on path {}", &compose_file_path))?;

    let yaml_file_contents = String::from_utf8_lossy(&file_contents).to_string();

    let compose_file_contents =
        YamlLoader::load_from_str(&yaml_file_contents).map_err(|_e| "Yaml File failed to parse")?;

    if compose_file_contents.is_empty() {
        return Err("Compose file is empty".into());
    }

    let spam_steps = &compose_file_contents[0]["spam"]["stages"];

    let spam_stages_and_scenarios: Vec<CompositeSpamConfiguration> = spam_steps
        .as_hash()
        .unwrap()
        .iter()
        .map(|value| {
            let (spam_stage, spam_objects_list) = value;
            CompositeSpamConfiguration {
                stage_name: spam_stage.as_str().unwrap().into(),
                spam_configs: spam_objects_list
                    .as_vec()
                    .unwrap()
                    .iter()
                    .map(|i| get_spam_object(i).unwrap())
                    .collect::<Vec<SpamCommandArgs>>(),
            }
        })
        .collect();

    Ok(spam_stages_and_scenarios)
}

fn get_spam_object(spam_object_yaml: &Yaml) -> Result<SpamCommandArgs, Box<dyn Error>> {
    // One of TPS or TPB should exist. need one of these for sure else an error.
    let spam_object = spam_object_yaml
        .clone()
        .into_hash()
        .ok_or(format!("Malformed scenario {spam_object_yaml:?}"))?;


    let testfile_path = get_testfile(&spam_object)?;

    let env_variables = get_env_variables(&spam_object)?;

    let rpc_url = get_rpc_url(&spam_object)?;

    let min_balance = get_min_balance(&spam_object)?;

    let private_keys = get_private_keys(&spam_object)?;

    let tx_type = get_tx_type(&spam_object)?;


    let txs_per_second = match spam_object.get(&Yaml::String("tps".into())) {
        Some(tps_value) => tps_value.as_i64().map(|value| value as u64),
        None => None,
    };

    let txs_per_block = match spam_object.get(&Yaml::String("tpb".into())) {
        Some(tps_value) => tps_value.as_i64().map(|value| value as u64),
        None => None,
    };

    if txs_per_second.is_some() && txs_per_block.is_some() {
        return Err("Both tps and tpb can't be specified in the spam object.".into());
    }

    if txs_per_second.is_none() && txs_per_block.is_none() {
        return Err("Neither tps nor tpb is specified.".into());
    }


    let timeout_secs = match spam_object.get(&Yaml::String("timeout".into())) {
        Some(timeout_value) => match timeout_value.as_i64() {
            Some(value) => value as u64,
            None => return Err("Invalid value type for timeout".into()),
        },
        None => 12,
    };

    let duration = match spam_object.get(&Yaml::String("duration".into())) {
        Some(duration_value) => match duration_value.as_i64() {
            Some(value) => value as u64,
            None => return Err("Invalid value type for 'duration'".into()),
        },
        None => 1,
    };

    let builder_url = match spam_object.get(&Yaml::String("builder_url".into())) {
        Some(builder_url_value) => match builder_url_value.as_str() {
            Some(value) => Some(String::from_str(value)?),
            None => None,
        },
        None => None,
    };

    let loops = match spam_object.get(&Yaml::String("loops".into())) {
        Some(loops_value) => match loops_value.as_i64() {
            Some(value) => Some(value as u64),
            None => return Err("Invalid data type for value of 'loops'".into()),
        },
        None => Some(1),
    };

    let spam_command_args = SpamCommandArgs {
        scenario: spam::SpamScenario::Testfile(testfile_path),
        txs_per_block,
        txs_per_second,
        rpc_url,
        builder_url,
        timeout_secs,
        duration,
        env: env_variables,
        private_keys,
        min_balance,
        tx_type,
        loops,

        // TODO: Need more knowledge
        seed: "0x01".into(),
        disable_reporting: true,
        engine_params: EngineParams {
            engine_provider: None,
            call_fcu: false,
        },
        gas_price_percent_add: None,
    };
    Ok(spam_command_args)
}

pub fn get_rpc_url(yaml_object: &LinkedHashMap<Yaml, Yaml>) -> Result<String, Box<dyn Error>> {
    match yaml_object.get(&Yaml::String("rpc_url".into())) {
        Some(rpc_url_value) => {
            if let Some(url) = rpc_url_value.as_str() {
                Ok(String::from(url))
            } else {
                Err("Invalid type value for 'rpc_url'".into())
            }
        }
        None => Ok(String::from("http://localhost:8545")),
    }
}

pub fn get_min_balance(yaml_object: &LinkedHashMap<Yaml, Yaml>) -> Result<String, Box<dyn Error>> {
    match yaml_object.get(&Yaml::String("min_balance".into())) {
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
                        Err(format!("Invalid min_balance string value: '{str_val:?}'").into())
                    }
                }
                _ => Err("Invalid min_balance string value".into()),
            }
        }
        None => Ok("0.01".to_string()),
    }
}

pub fn get_private_keys(
    yaml_object: &LinkedHashMap<Yaml, Yaml>,
) -> Result<Option<Vec<String>>, Box<dyn Error>> {
    match yaml_object.get(&Yaml::String("private_keys".into())) {
        Some(value) => match value {
            Yaml::Array(priv_keys) => {
                let mut temp_private_keys = vec![];
                for item in priv_keys {
                    if let Some(p_key) = item.as_str() {
                        temp_private_keys.push(p_key.to_string());
                    }
                }
                Ok(Some(temp_private_keys))
            }
            _ => Err(format!("Invalid env_value type: {value:?}").into()),
        },
        None => Ok(None),
    }
}

pub fn get_tx_type(
    yaml_object: &LinkedHashMap<Yaml, Yaml>,
) -> Result<alloy::consensus::TxType, Box<dyn Error>> {
    match yaml_object.get(&Yaml::String("tx_type".into())) {
        Some(value) => match value {
            Yaml::String(tx_type_string) => match tx_type_string.as_str() {
                "legacy" => Ok(alloy::consensus::TxType::Legacy),
                "eip1559" => Ok(alloy::consensus::TxType::Eip1559),
                _ => {
                    Err(format!("Invalid Value for 'tx_type' = {}", tx_type_string.as_str()).into())
                }
            },
            _ => Err(format!("Invalid type value for 'tx_type' = {value:?}").into()),
        },
        None => Ok(alloy::consensus::TxType::Eip1559),
    }
}

pub fn get_env_variables(
    yaml_object: &LinkedHashMap<Yaml, Yaml>,
) -> Result<Option<Vec<(String, String)>>, Box<dyn Error>> {
    match yaml_object.get(&Yaml::String("env".into())) {
        Some(value) => match value {
            Yaml::Array(env_vars) => {
                let mut temp_env_variables = vec![];
                for item in env_vars {
                    if let Some(env_value_pair) = item.as_str() {
                        temp_env_variables.push(cli_env_vars_parser(env_value_pair)?);
                    }
                }
                Ok(Some(temp_env_variables))
            }
            _ => Err(format!("Invalid env_value type: {:?}", &value).into()),
        },
        None => Ok(None),
    }
}

pub fn get_testfile(yaml_object: &LinkedHashMap<Yaml, Yaml>) -> Result<String, Box<dyn Error>> {
    match yaml_object.get(&Yaml::String("testfile".into())) {
        Some(testfile_path_value) => match testfile_path_value.as_str() {
            Some(value) => Ok(value.to_owned()),
            None => Err("invalid type of value for 'scenario'".into()),
        },
        None => Err("'scenario' missing in the spam configuration".into()),
    }
}

pub struct ComposeFile {
    pub setup: Option<Vec<ComposeFileScenario>>,
    pub spam: Option<Vec<CompositeSpamConfiguration>>,
}

impl ComposeFile {
    pub fn init_from_path(file_path: String) -> Result<Self, Box<dyn Error>> {
        let setup_config = Some(get_setup_from_compose_file(file_path.clone())?);
        let spam_config = Some(get_spam_configuration_from_compose_file(file_path.clone())?);
        Ok(ComposeFile {
            setup: setup_config,
            spam: spam_config,
        })
    }
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
                .contains("'scenario' missing in the spam configuration")),
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
        assert_eq!(
            scenarios[0].config.tx_type,
            alloy::consensus::TxType::Eip1559
        );
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
        assert_eq!(
            result.err().unwrap().to_string(),
            "Invalid Value for 'tx_type' = invalid_type".to_string()
        )
    }

    // Done
    #[test]
    fn test_valid_env_variables() -> Result<(), Box<dyn std::error::Error>> {
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

        assert_eq!(
            result[0].config.env.as_ref().unwrap()[0].0.to_string(),
            "KEY1".to_string()
        );
        assert_eq!(
            result[0].config.env.as_ref().unwrap()[0].1,
            "VALUE1".to_string()
        );
        assert_eq!(
            result[0].config.env.as_ref().unwrap()[1].0,
            "KEY2".to_string()
        );
        assert_eq!(
            result[0].config.env.as_ref().unwrap()[1].1,
            "VALUE2".to_string()
        );

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
