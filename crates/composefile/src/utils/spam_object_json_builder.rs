use crate::types::SpamCommandArgsJsonAdapter;
use std::{error::Error, str::FromStr};
use yaml_rust2::Yaml;

use super::{
    get_env_variables, get_min_balance, get_private_keys, get_rpc_url, get_testfile, get_tx_type,
};

pub fn spam_object_json_builder(
    spam_object_yaml: &Yaml,
) -> Result<SpamCommandArgsJsonAdapter, Box<dyn Error>> {
    // One of TPS or TPB should exist. need one of these for sure else an error.
    let spam_object = spam_object_yaml
        .clone()
        .into_hash()
        .ok_or(format!("Malformed spam_object {spam_object_yaml:?}"))?;

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

    let spam_command_args = SpamCommandArgsJsonAdapter {
        scenario: testfile_path,
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
    };
    Ok(spam_command_args)
}

#[cfg(test)]
mod spam_object_json_builder_tests {
    use hashlink::LinkedHashMap;
    use yaml_rust2::{Yaml, YamlLoader};

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
    fn test_valid_with_tps() {
        let yaml_str = r#"
            testfile: ./scenarios/spam.toml
            rpc_url: http://localhost:8545
            min_balance: "42"
            env:
              - A=1
              - B=2
            private_keys:
            - 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80
            - 0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d
            tx_type: eip1559
            tps: 100
            timeout: 30
            duration: 60
            builder_url: http://builder.local
            loops: 5
        "#;

        let yaml = parse_yaml(yaml_str);
        let result = spam_object_json_builder(&Yaml::Hash(yaml.clone()));

        assert!(result.is_ok());
        let json = result.unwrap();

        assert_eq!(json.scenario, "./scenarios/spam.toml");
        assert_eq!(json.rpc_url, "http://localhost:8545");
        assert_eq!(json.min_balance, "42");
        assert_eq!(
            json.env,
            Some(vec![
                ("A".to_string(), "1".to_string()),
                ("B".to_string(), "2".to_string())
            ])
        );
        assert_eq!(
            json.private_keys,
            Some(vec![
                "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80".to_string(),
                "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d".to_string()
            ])
        );
        // );
        assert_eq!(json.tx_type, "eip1559");
        assert_eq!(json.txs_per_second, Some(100));
        assert_eq!(json.txs_per_block, None);
        assert_eq!(json.timeout_secs, 30);
        assert_eq!(json.duration, 60);
        assert_eq!(json.builder_url, Some("http://builder.local".to_string()));
        assert_eq!(json.loops, Some(5));
    }

    #[test]
    fn test_valid_with_tpb_and_defaults() {
        let yaml_str = r#"
            testfile: ./scenarios/spam.toml
            min_balance: 10
            tpb: 20
        "#;

        let yaml = parse_yaml(yaml_str);
        let result = spam_object_json_builder(&Yaml::Hash(yaml.clone()));

        assert!(result.is_ok());
        let json = result.unwrap();

        assert_eq!(json.scenario, "./scenarios/spam.toml");
        assert_eq!(json.txs_per_block, Some(20));
        assert_eq!(json.txs_per_second, None);
        assert_eq!(json.timeout_secs, 12); // default
        assert_eq!(json.duration, 1); // default
        assert_eq!(json.rpc_url, "http://localhost:8545"); // default
        assert_eq!(json.env, None);
        assert_eq!(json.private_keys, None);
        assert_eq!(json.tx_type, "eip1559"); // default
        assert_eq!(json.loops, Some(1)); // default
    }

    #[test]
    fn test_error_on_both_tps_and_tpb() {
        let yaml_str = r#"
            testfile: test.toml
            tps: 100
            tpb: 200
        "#;

        let yaml = parse_yaml(yaml_str);
        let result = spam_object_json_builder(&Yaml::Hash(yaml));

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Both tps and tpb can't be specified"));
    }

    #[test]
    fn test_error_on_neither_tps_nor_tpb() {
        let yaml_str = r#"
            testfile: test.toml
        "#;

        let yaml = parse_yaml(yaml_str);
        let result = spam_object_json_builder(&Yaml::Hash(yaml));

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Neither tps nor tpb is specified"));
    }

    #[test]
    fn test_error_on_invalid_timeout_type() {
        let yaml_str = r#"
            testfile: test.toml
            tps: 10
            timeout: "wrong"
        "#;

        let yaml = parse_yaml(yaml_str);
        let result = spam_object_json_builder(&Yaml::Hash(yaml));

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid value type for timeout"));
    }

    #[test]
    fn test_error_on_invalid_loops_type() {
        let yaml_str = r#"
            testfile: test.toml
            tps: 10
            loops: "many"
        "#;

        let yaml = parse_yaml(yaml_str);
        let result = spam_object_json_builder(&Yaml::Hash(yaml));

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid data type for value of 'loops'"));
    }

    #[test]
    fn test_error_on_non_hash_input() {
        let yaml = Yaml::String("not a hash".into());

        let result = spam_object_json_builder(&yaml);

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Malformed spam_object"));
    }
}
