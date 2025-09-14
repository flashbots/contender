use crate::types::SetupCommandArgsJsonAdapter;
use std::error::Error;
use yaml_rust2::Yaml;

use super::{get_env_variables, get_min_balance, get_rpc_url, get_testfile, get_tx_type};

pub fn setup_object_json_builder(
    setup_object_yaml: &Yaml,
) -> Result<SetupCommandArgsJsonAdapter, Box<dyn Error>> {
    let setup_object = setup_object_yaml
        .clone()
        .into_hash()
        .ok_or(format!("Malformed setup_object {setup_object_yaml:?}"))?;

    let testfile = get_testfile(&setup_object)?;

    let rpc_url = get_rpc_url(&setup_object)?;

    let min_balance = get_min_balance(&setup_object)?;

    let env_variables = get_env_variables(&setup_object)?;

    let tx_type = get_tx_type(&setup_object)?;

    let seed: Option<String> = match setup_object.get(&Yaml::String("seed".into())) {
        Some(seed_value) => match seed_value.as_str() {
            Some(value) => Some(value.to_owned()),
            None => return Err("Invalid data type for value of 'seed'".into()),
        },
        None => None,
    };

    let call_forkchoice = match setup_object.get(&Yaml::String("call_forkchoice".into())) {
        Some(fcu) => match fcu.as_bool() {
            Some(value) => value,
            None => return Err("Invalid data type for 'call_forkchoice'".into()),
        },
        None => false,
    };

    let use_op = match setup_object.get(&Yaml::String("optimism".into())) {
        Some(optimism) => match optimism.as_bool() {
            Some(value) => value,
            None => return Err("Invalid data type for 'optimism'".into()),
        },
        None => false,
    };

    let auth_rpc_url = match setup_object.get(&Yaml::String("auth_rpc_url".into())) {
        Some(auth_rpc_url_value) => match auth_rpc_url_value.as_str() {
            Some(value) => Some(value.to_owned()),
            None => return Err("Invalid data type for value of 'auth_rpc_url'".into()),
        },
        None => None,
    };

    let jwt_secret = match setup_object.get(&Yaml::String("auth_rpc_url".into())) {
        Some(jwt_secret_value) => match jwt_secret_value.as_str() {
            Some(value) => Some(value.to_owned()),
            None => return Err("Invalid data type for value of 'jwt_secret'".into()),
        },
        None => None,
    };

    Ok(SetupCommandArgsJsonAdapter {
        testfile,
        rpc_url,
        min_balance,
        env: env_variables,
        tx_type,
        private_keys: None,
        seed,
        call_fcu: call_forkchoice,
        use_op,
        auth_rpc_url,
        jwt_secret,
    })
}

#[cfg(test)]
mod setup_object_json_builder_tests {
    use hashlink::LinkedHashMap;
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
    fn test_valid_setup_object_yaml() {
        let yaml_str = r#"
            testfile: ./scenarios/uniV2.toml
            rpc_url: http://localhost:8545
            min_balance: "11"
            env:
              - Key1=Valu1
              - Key2=Valu2
            private_keys:
              - 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80
              - 0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d
            tx_type: eip1559
        "#;

        let yaml = parse_yaml(yaml_str);
        let result = setup_object_json_builder(&Yaml::Hash(yaml.clone()));

        assert!(result.is_ok());

        let json = result.unwrap();

        assert_eq!(json.testfile, "./scenarios/uniV2.toml");
        assert_eq!(json.rpc_url, "http://localhost:8545".to_string());
        assert_eq!(json.min_balance, "11");

        assert_eq!(
            json.env,
            Some(vec![
                ("Key1".to_string(), "Valu1".to_string()),
                ("Key2".to_string(), "Valu2".to_string())
            ])
        );

        assert_eq!(json.tx_type, "eip1559");
    }

    #[test]
    fn test_missing_optional_fields() {
        let yaml_str = r#"
            testfile: ./scenarios/uniV2.toml
            min_balance: 5
        "#;

        let yaml = parse_yaml(yaml_str);
        let result = setup_object_json_builder(&Yaml::Hash(yaml));

        assert!(result.is_ok());

        let json = result.unwrap();
        assert_eq!(json.testfile, "./scenarios/uniV2.toml");
        assert_eq!(json.min_balance, "5");
        assert_eq!(json.rpc_url, "http://localhost:8545");
        assert_eq!(json.env, None);
        assert_eq!(json.private_keys, None);
        assert_eq!(json.tx_type, "eip1559");
    }

    #[test]
    fn test_invalid_yaml_structure() {
        let yaml = Yaml::String("not a hash".into());

        let result = setup_object_json_builder(&yaml);

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Malformed setup_object"));
    }
}
