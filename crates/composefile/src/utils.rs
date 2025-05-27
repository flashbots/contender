use hashlink::LinkedHashMap;
use std::{error::Error, str::FromStr};
use yaml_rust2::Yaml;

use crate::types::SpamCommandArgsJsonAdapter;

pub fn cli_env_vars_parser(s: &str) -> Result<(String, String), String> {
    let equal_sign_index = s.find('=').ok_or("Invalid KEY=VALUE: No \"=\" found")?;

    if equal_sign_index == 0 {
        return Err("Empty Key: No Key found".to_owned());
    }

    Ok((
        s[..equal_sign_index].to_string(),
        s[equal_sign_index + 1..].to_string(),
    ))
}

pub fn get_spam_object(
    spam_object_yaml: &Yaml,
) -> Result<SpamCommandArgsJsonAdapter, Box<dyn Error>> {
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

pub fn get_tx_type(yaml_object: &LinkedHashMap<Yaml, Yaml>) -> Result<String, Box<dyn Error>> {
    match yaml_object.get(&Yaml::String("tx_type".into())) {
        Some(value) => match value {
            Yaml::String(tx_type_string) => match tx_type_string.as_str() {
                "legacy" => Ok("legacy".into()),
                "eip1559" => Ok("eip1559".into()),
                _ => {
                    Err(format!("Invalid Value for 'tx_type' = {}", tx_type_string.as_str()).into())
                }
            },
            _ => Err(format!("Invalid type value for 'tx_type' = {value:?}").into()),
        },
        None => Ok("eip1559".into()),
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
