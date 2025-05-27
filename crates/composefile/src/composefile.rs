use std::{error::Error, fs, str::FromStr};
use yaml_rust2::{Yaml, YamlLoader};

use crate::{
    types::{SetupCommandArgsJsonAdapter, SpamCommandArgsJsonAdapter},
    utils::{
        get_env_variables, get_min_balance, get_private_keys, get_rpc_url, get_spam_object,
        get_testfile, get_tx_type,
    },
};

#[derive(Debug)]
pub struct ComposeFile {
    pub yaml: Yaml,
}

#[derive(Clone)]
pub struct ComposeFileScenario {
    pub name: String,
    pub config: SetupCommandArgsJsonAdapter,
}

pub struct CompositeSpamConfiguration {
    pub stage_name: String,
    pub spam_configs: Vec<SpamCommandArgsJsonAdapter>,
}

impl ComposeFile {
    pub fn init_from_path(file_path: String) -> Result<Self, Box<dyn Error>> {
        let file_contents = fs::read(&file_path)
            .map_err(|_e| format!("Can't read the file on path {}", &file_path))?;

        let yaml_file_contents = String::from_utf8_lossy(&file_contents).to_string();

        let compose_file_contents = YamlLoader::load_from_str(&yaml_file_contents)
            .map_err(|_e| "Yaml File failed to parse")?;

        if compose_file_contents.is_empty() {
            return Err("Compose file is empty".into());
        }

        Ok(ComposeFile {
            yaml: Yaml::Array(compose_file_contents),
        })
    }

    pub fn get_setup_config(&self) -> Result<Vec<ComposeFileScenario>, Box<dyn Error>> {
        let contracts_list = &self.yaml[0]["setup"];

        let mut setup_scenarios = vec![];

        let contracts_hash = match contracts_list.as_hash() {
            Some(val) => val,
            None => return Ok(Vec::new()),
        };

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

            let name = match scenario_name.as_str() {
                Some(val) => String::from_str(val),
                None => return Err(format!("Not a valid scenario name '{scenario_name:?}'").into()),
            }?;

            setup_scenarios.push(ComposeFileScenario {
                name,
                config: SetupCommandArgsJsonAdapter {
                    testfile,
                    rpc_url,
                    min_balance,
                    env: env_variables,
                    tx_type,
                    private_keys,
                    // TODO: Hardcoded parameters for now, need more understanding on where to get these from
                    // seed,
                    // engine_params
                },
            });
        }
        Ok(setup_scenarios)
    }

    pub fn get_spam_config(&self) -> Result<Vec<CompositeSpamConfiguration>, Box<dyn Error>> {
        let spam_steps = &self.yaml[0]["spam"]["stages"];

        let spam_hash = spam_steps
            .as_hash()
            .ok_or("Failed to parse spam steps as hash")?;

        let spam_stages_and_scenarios: Vec<CompositeSpamConfiguration> = spam_hash
            .iter()
            .map(
                |value| -> Result<CompositeSpamConfiguration, Box<dyn Error>> {
                    let (spam_stage, spam_objects_list) = value;

                    let stage_name = spam_stage
                        .as_str()
                        .ok_or("Failed to parse stage name as string")?
                        .into();

                    let spam_objects_vec = spam_objects_list
                        .as_vec()
                        .ok_or("Failed to parse spam objects as vector")?;

                    let spam_configs: Result<Vec<SpamCommandArgsJsonAdapter>, Box<dyn Error>> =
                        spam_objects_vec
                            .iter()
                            .map(|i| get_spam_object(i))
                            .collect();

                    Ok(CompositeSpamConfiguration {
                        stage_name,
                        spam_configs: spam_configs?,
                    })
                },
            )
            .collect::<Result<Vec<_>, _>>()?;

        Ok(spam_stages_and_scenarios)
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
        let file = ComposeFile::init_from_path("./non-existent-scene.yml".to_string());

        match file {
            Ok(_) => panic!("Expected an error, but got Ok(_)"),
            Err(e) => {
                assert_eq!(
                    e.to_string(),
                    "Can't read the file on path ./non-existent-scene.yml"
                );
            }
        }

        Ok(())
    }

    #[test]
    fn test_basic_valid_yaml() -> Result<(), Box<dyn Error>> {
        let yaml_content = r#"
setup:
  scenario1:
    testfile: "./scenarios/simpler.toml"
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
        let yaml_file =
            ComposeFile::init_from_path(temp_file.path().to_string_lossy().into_owned())?;
        let scenarios = yaml_file.get_setup_config()?;

        assert_eq!(scenarios.len(), 1);
        assert_eq!(scenarios[0].name, "scenario1");
        assert_eq!(scenarios[0].config.testfile, "./scenarios/simpler.toml");
        assert_eq!(scenarios[0].config.rpc_url, "http://localhost:9545");
        assert_eq!(scenarios[0].config.min_balance, "5");
        assert_eq!(scenarios[0].config.tx_type, "eip1559".to_string());

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

    #[test]
    fn test_missing_testfile() -> Result<(), Box<dyn Error>> {
        let yaml_content = r#"
setup:
  missing_testfile:
    rpc_url: "http://localhost:8545"
"#;
        let temp_file = create_temp_yaml_file(yaml_content)?;

        let yaml_file =
            ComposeFile::init_from_path(temp_file.path().to_string_lossy().into_owned())?;
        let result = yaml_file.get_setup_config();

        assert!(result.is_err());
        match result {
            Err(e) => assert!(e
                .to_string()
                .contains("'scenario' missing in the spam configuration")),
            _ => panic!("Expected an error for missing testfile"),
        }
        Ok(())
    }

    #[test]
    fn test_default_values() -> Result<(), Box<dyn Error>> {
        let yaml_content = r#"
setup:
  scenario1:
    testfile: "./scenarios/simpler.toml"
"#;
        let temp_file = create_temp_yaml_file(yaml_content)?;
        let compose_file =
            ComposeFile::init_from_path(temp_file.path().to_string_lossy().to_string())?;
        let scenarios = compose_file.get_setup_config()?;

        assert_eq!(scenarios.len(), 1);
        assert_eq!(scenarios[0].name, "scenario1");
        assert_eq!(scenarios[0].config.testfile, "./scenarios/simpler.toml");
        assert_eq!(scenarios[0].config.rpc_url, "http://localhost:8545");
        assert_eq!(scenarios[0].config.min_balance, "0.01"); // Default value
        assert_eq!(scenarios[0].config.tx_type, "eip1559".to_string());
        assert_eq!(scenarios[0].config.env, None);
        assert_eq!(scenarios[0].config.private_keys, None);
        Ok(())
    }

    #[test]
    fn test_legacy_tx_type() -> Result<(), Box<dyn Error>> {
        let yaml_content = r#"
setup:
  legacy_scenario:
    testfile: "./scenarios/simpler.toml"
    tx_type: "legacy"
"#;

        let temp_file = create_temp_yaml_file(yaml_content)?;
        let compose_file =
            ComposeFile::init_from_path(temp_file.path().to_string_lossy().to_string())?;
        let scenarios = compose_file.get_setup_config()?;

        assert_eq!(scenarios.len(), 1);
        assert_eq!(scenarios[0].config.tx_type, "legacy".to_string());

        Ok(())
    }

    #[test]
    fn test_eip1559_tx_type() -> Result<(), Box<dyn Error>> {
        let yaml_content = r#"
setup:
  legacy_scenario:
    testfile: "./scenarios/simpler.toml"
    tx_type: eip1559
"#;

        let temp_file = create_temp_yaml_file(yaml_content)?;
        let compose_file =
            ComposeFile::init_from_path(temp_file.path().to_string_lossy().to_string())?;
        let scenarios = compose_file.get_setup_config()?;

        assert_eq!(scenarios[0].config.tx_type, "eip1559".to_string());

        Ok(())
    }

    // Done
    #[test]
    fn test_invalid_tx_type() -> Result<(), Box<dyn Error>> {
        let yaml_content = r#"
setup:
  invalid_scenario:
    testfile: "./scenarios/simpler.toml"
    tx_type: "invalid_type"
"#;

        let temp_file = create_temp_yaml_file(yaml_content)?;
        let compose_file =
            ComposeFile::init_from_path(temp_file.path().to_string_lossy().to_string())?;
        let result = compose_file.get_setup_config();

        match result {
            Ok(_) => return Err("Didn't expect test to pass".into()),
            Err(e) => {
                assert_eq!(
                    e.to_string(),
                    "Invalid Value for 'tx_type' = invalid_type".to_string()
                )
            }
        }
        Ok(())
    }

    // Done
    #[test]
    fn test_valid_env_variables() -> Result<(), Box<dyn std::error::Error>> {
        let yaml_content = r#"
setup:
  invalid_scenario:
    testfile: "./scenarios/simpler.toml"
    env:
        - KEY1=VALUE1
        - KEY2=VALUE2
"#;

        let temp_file = create_temp_yaml_file(yaml_content)?;
        let compose_file =
            ComposeFile::init_from_path(temp_file.path().to_string_lossy().to_string())?;
        let result = compose_file.get_setup_config()?;

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
    fn test_malformed_yaml() -> Result<(), Box<dyn Error>> {
        let yaml_content = r#"
setup:
  malformed:
    testfile: "test.js"
    min_balance: ]
"#;

        let temp_file = create_temp_yaml_file(yaml_content)?;
        let compose_file =
            ComposeFile::init_from_path(temp_file.path().to_string_lossy().to_string());
        match compose_file {
            Ok(_) => panic!("Expected an error, but got Ok(_)"),
            Err(e) => {
                assert!(e.to_string().contains("Yaml File failed to parse"));
            }
        };
        Ok(())
    }

    #[test]
    fn test_empty_yaml() -> Result<(), Box<dyn Error>> {
        let yaml_content = "";
        let temp_file = create_temp_yaml_file(yaml_content)?;
        let compose_file =
            ComposeFile::init_from_path(temp_file.path().to_string_lossy().to_string());
        match compose_file {
            Ok(_) => panic!("Expected an error, but got Ok(_)"),
            Err(e) => {
                assert!(e.to_string().contains("Compose file is empty"));
            }
        };

        Ok(())
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
        let compose_file =
            ComposeFile::init_from_path(temp_file.path().to_string_lossy().to_string())?;
        let scenarios = compose_file.get_setup_config()?;

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
        let compose_file =
            ComposeFile::init_from_path(temp_file.path().to_string_lossy().to_string())?;
        let scenarios = compose_file.get_setup_config()?;

        let private_keys = scenarios[0].config.private_keys.as_ref().unwrap();
        assert_eq!(private_keys.len(), 0);

        let env_vars = scenarios[0].config.env.as_ref().unwrap();
        assert_eq!(env_vars.len(), 0);

        Ok(())
    }
}
