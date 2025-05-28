use hashlink::LinkedHashMap;
use std::error::Error;
use yaml_rust2::Yaml;

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

#[cfg(test)]
mod get_min_balance_tests {
    use super::*;

    #[test]
    fn test_real_min_balance() {
        let mut yaml = LinkedHashMap::new();
        yaml.insert(
            Yaml::String("min_balance".into()),
            Yaml::Real("0.05".into()),
        );
        let result = get_min_balance(&yaml);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "0.05");
    }

    #[test]
    fn test_integer_min_balance() {
        let mut yaml = LinkedHashMap::new();
        yaml.insert(Yaml::String("min_balance".into()), Yaml::Integer(10));
        let result = get_min_balance(&yaml);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "10");
    }

    #[test]
    fn test_string_numeric_min_balance() {
        let mut yaml = LinkedHashMap::new();
        yaml.insert(
            Yaml::String("min_balance".into()),
            Yaml::String("100.0".into()),
        );
        let result = get_min_balance(&yaml);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "100.0");
    }

    #[test]
    fn test_string_non_numeric_min_balance() {
        let mut yaml = LinkedHashMap::new();
        yaml.insert(
            Yaml::String("min_balance".into()),
            Yaml::String("abc".into()),
        );
        let result = get_min_balance(&yaml);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid min_balance string value"));
    }

    #[test]
    fn test_invalid_type_min_balance() {
        let mut yaml = LinkedHashMap::new();
        yaml.insert(Yaml::String("min_balance".into()), Yaml::Boolean(true));
        let result = get_min_balance(&yaml);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Invalid min_balance string value"
        );
    }

    #[test]
    fn test_missing_min_balance() {
        let yaml = LinkedHashMap::new();
        let result = get_min_balance(&yaml);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "0.01");
    }
}
