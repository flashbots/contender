use hashlink::LinkedHashMap;
use std::error::Error;
use yaml_rust2::Yaml;

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

#[cfg(test)]
mod get_tx_type_tests {
    use super::*;

    #[test]
    fn test_valid_legacy_tx_type() {
        let mut yaml = LinkedHashMap::new();
        yaml.insert(
            Yaml::String("tx_type".into()),
            Yaml::String("legacy".into()),
        );

        let result = get_tx_type(&yaml);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "legacy");
    }

    #[test]
    fn test_valid_eip1559_tx_type() {
        let mut yaml = LinkedHashMap::new();
        yaml.insert(
            Yaml::String("tx_type".into()),
            Yaml::String("eip1559".into()),
        );

        let result = get_tx_type(&yaml);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "eip1559");
    }

    #[test]
    fn test_invalid_string_tx_type() {
        let mut yaml = LinkedHashMap::new();
        yaml.insert(Yaml::String("tx_type".into()), Yaml::String("foo".into()));

        let result = get_tx_type(&yaml);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Invalid Value for 'tx_type' = foo"
        );
    }

    #[test]
    fn test_invalid_type_tx_type() {
        let mut yaml = LinkedHashMap::new();
        yaml.insert(Yaml::String("tx_type".into()), Yaml::Integer(42));

        let result = get_tx_type(&yaml);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .starts_with("Invalid type value for 'tx_type'"));
    }

    #[test]
    fn test_missing_tx_type_defaults_to_eip1559() {
        let yaml = LinkedHashMap::new();

        let result = get_tx_type(&yaml);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "eip1559");
    }
}
