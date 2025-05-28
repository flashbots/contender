use std::{error::Error, fs, str::FromStr};
use yaml_rust2::{Yaml, YamlLoader};

use crate::{
    types::{SetupCommandArgsJsonAdapter, SpamCommandArgsJsonAdapter},
    utils::{setup_object_json_builder, spam_object_json_builder},
};

#[derive(Debug)]
pub struct ComposeFile {
    pub yaml: Yaml,
}

#[derive(Clone, Debug)]
pub struct ComposeFileScenario {
    pub name: String,
    pub config: SetupCommandArgsJsonAdapter,
}

#[derive(Debug)]
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
        let setup_scenarios_from_file = &self.yaml[0]["setup"];

        let mut setup_scenarios = vec![];

        let setup_scenarios_object = match setup_scenarios_from_file.as_hash() {
            Some(val) => val,
            None => return Ok(Vec::new()),
        };

        for (scenario_name, setup_command_params) in setup_scenarios_object.into_iter() {
            setup_scenarios.push(ComposeFileScenario {
                name: match scenario_name.as_str() {
                    Some(val) => String::from_str(val)?,
                    None => {
                        return Err(format!("Not a valid scenario name '{scenario_name:?}'").into())
                    }
                },
                config: setup_object_json_builder(setup_command_params)?,
            })
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
                            .map(|item| spam_object_json_builder(item))
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
    fn test_malformed_yaml() -> Result<(), Box<dyn Error>> {
        let yaml_content = r#"
setup:
  malformed:
    testfile: "./scenarios/simpler.toml"
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
    fn test_invalid_setup_key() -> Result<(), Box<dyn Error>> {
        let yaml_content = r#"
setup:
  123:
    testfile: "./scenarios/simpler.toml"
"#;
        let temp_file = create_temp_yaml_file(yaml_content)?;
        let file = ComposeFile::init_from_path(temp_file.path().to_string_lossy().into_owned())?;
        let result = file.get_setup_config();

        assert!(result.is_err());
        assert!(result
            .as_ref()
            .unwrap_err()
            .to_string()
            .contains("Not a valid scenario name"));

        Ok(())
    }

    #[test]
    fn test_get_spam_config_invalid_structure() -> Result<(), Box<dyn Error>> {
        let yaml_str = r#"
            setup: {}
            spam:
              stages: true
        "#;

        let temp_file = create_temp_yaml_file(yaml_str)?;
        let file =
            ComposeFile::init_from_path(temp_file.path().to_string_lossy().into_owned()).unwrap();
        let result = file.get_spam_config();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Failed to parse spam steps as hash"));

        Ok(())
    }

    #[test]
    fn test_valid_spam_config_with_multiple_stages() -> Result<(), Box<dyn Error>> {
        let yaml_content = r#"
spam:
  stages:
    stage1:
      - testfile: "./scenarios/simpler.toml"
        tps: 10
      - testfile: "./scenarios/simpler.toml"
        tps: 7
    stage2:
      - testfile: "./scenarios/simpler.toml"
        tps: 20
"#;

        let temp_file = create_temp_yaml_file(yaml_content)?;
        let file = ComposeFile::init_from_path(temp_file.path().to_string_lossy().into_owned())?;
        let spam_config = file.get_spam_config()?;

        assert_eq!(spam_config.len(), 2);
        assert_eq!(spam_config[0].stage_name, "stage1");
        assert_eq!(spam_config[1].stage_name, "stage2");

        Ok(())
    }

    #[test]
    fn test_spam_stage_not_a_list() -> Result<(), Box<dyn Error>> {
        let yaml_content = r#"
spam:
  stages:
    stage1: "not-a-list"
"#;

        let temp_file = create_temp_yaml_file(yaml_content)?;
        let file = ComposeFile::init_from_path(temp_file.path().to_string_lossy().into_owned())?;
        let result = file.get_spam_config();

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Failed to parse spam objects as vector"));

        Ok(())
    }
}
