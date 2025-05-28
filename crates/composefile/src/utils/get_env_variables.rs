use hashlink::LinkedHashMap;
use std::error::Error;
use yaml_rust2::Yaml;

pub fn get_env_variables(
    yaml_object: &LinkedHashMap<Yaml, Yaml>,
) -> Result<Option<Vec<(String, String)>>, Box<dyn Error>> {
    match yaml_object.get(&Yaml::String("env".into())) {
        Some(value) => match value {
            Yaml::Array(env_vars) => {
                let mut temp_env_variables = vec![];
                for item in env_vars {
                    match item {
                        Yaml::String(env_value_pair) => {
                            temp_env_variables.push(cli_env_vars_parser(env_value_pair)?);
                        }
                        _ => return Err(format!("Illegal item {item:?}").into()),
                    };
                }
                Ok(Some(temp_env_variables))
            }
            _ => Err(format!("Invalid env_value type: {:?}", &value).into()),
        },
        None => Ok(None),
    }
}

// Same as one in CLI. Tests for this function exist on contender_cli module
fn cli_env_vars_parser(s: &str) -> Result<(String, String), String> {
    let equal_sign_index = s.find('=').ok_or("Invalid KEY=VALUE: No \"=\" found")?;

    if equal_sign_index == 0 {
        return Err("Empty Key: No Key found".to_owned());
    }

    Ok((
        s[..equal_sign_index].to_string(),
        s[equal_sign_index + 1..].to_string(),
    ))
}

#[cfg(test)]
mod get_env_variables_tests {
    use yaml_rust2::YamlLoader;

    use super::*;

    fn parse_yaml(input: &str) -> LinkedHashMap<Yaml, Yaml> {
        YamlLoader::load_from_str(input)
            .unwrap()
            .remove(0)
            .as_hash()
            .unwrap()
            .to_owned()
    }

    #[test]
    fn test_valid_env_variables() {
        let yaml_str = r#"
            testfile: ./scenarios/uniV2.toml
            env:
              - FOO=bar
              - BAZ=qux
        "#;

        let result = get_env_variables(&parse_yaml(yaml_str));
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            Some(vec![
                ("FOO".to_string(), "bar".to_string()),
                ("BAZ".to_string(), "qux".to_string()),
            ])
        );
    }

    #[test]
    fn test_env_with_invalid_string_format() {
        let yaml_str = r#"
            testfile: ./scenarios/uniV2.toml
            env:
              - FOO=bar
              - INVALIDENTRY
        "#;

        let result = get_env_variables(&parse_yaml(yaml_str));
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid KEY=VALUE"));
    }

    #[test]
    fn test_env_with_non_string_items() {
        let yaml_str = r#"
            testfile: ./scenarios/uniV2.toml
            env:
              - 123
              - true
        "#;

        let result = get_env_variables(&parse_yaml(yaml_str));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Illegal item"));
    }

    #[test]
    fn test_env_field_with_invalid_type() {
        let yaml_str = r#"
            testfile: ./scenarios/uniV2.toml
            env: FOO=bar
        "#;
        let result = get_env_variables(&parse_yaml(yaml_str));
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .starts_with("Invalid env_value type"));
    }

    #[test]
    fn test_missing_env_field() {
        let yaml_str = r#"
            testfile: ./scenarios/uniV2.toml
        "#;

        let result = get_env_variables(&parse_yaml(yaml_str));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }
}
