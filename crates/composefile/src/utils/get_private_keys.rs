use hashlink::LinkedHashMap;
use std::error::Error;
use yaml_rust2::Yaml;

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

#[cfg(test)]
mod get_private_keys_tests {
    use super::*;

    #[test]
    fn test_valid_private_keys_array() {
        let mut yaml = LinkedHashMap::new();
        let keys = vec![
            Yaml::String("key1".into()),
            Yaml::String("key2".into()),
            Yaml::String("key3".into()),
        ];
        yaml.insert(Yaml::String("private_keys".into()), Yaml::Array(keys));

        let result = get_private_keys(&yaml);
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            Some(vec![
                "key1".to_string(),
                "key2".to_string(),
                "key3".to_string()
            ])
        );
    }

    #[test]
    fn test_array_with_mixed_types() {
        let mut yaml = LinkedHashMap::new();
        let keys = vec![
            Yaml::String("valid_key".into()),
            Yaml::Integer(123),
            Yaml::Boolean(true),
        ];
        yaml.insert(Yaml::String("private_keys".into()), Yaml::Array(keys));

        let result = get_private_keys(&yaml);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some(vec!["valid_key".to_string()])); // Only string values are collected
    }

    #[test]
    fn test_invalid_type_for_private_keys() {
        let mut yaml = LinkedHashMap::new();
        yaml.insert(
            Yaml::String("private_keys".into()),
            Yaml::String("not-an-array".into()),
        );

        let result = get_private_keys(&yaml);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid env_value type"));
    }

    #[test]
    fn test_missing_private_keys() {
        let yaml = LinkedHashMap::new();
        let result = get_private_keys(&yaml);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }
}
