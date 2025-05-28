use hashlink::LinkedHashMap;
use std::error::Error;
use yaml_rust2::Yaml;

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

#[cfg(test)]
mod get_rpc_url_tests {
    use super::*;

    #[test]
    fn test_valid_rpc_url() {
        let mut yaml_object = LinkedHashMap::new();
        yaml_object.insert(
            Yaml::String("rpc_url".into()),
            Yaml::String("https://rpc.com".into()),
        );

        let result = get_rpc_url(&yaml_object);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "https://rpc.com");
    }

    #[test]
    fn test_invalid_rpc_url_type() {
        let mut yaml_object = LinkedHashMap::new();
        yaml_object.insert(Yaml::String("rpc_url".into()), Yaml::Integer(12345));

        let result = get_rpc_url(&yaml_object);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Invalid type value for 'rpc_url'"
        );
    }

    #[test]
    fn test_missing_rpc_url() {
        let yaml_object = LinkedHashMap::new();

        let result = get_rpc_url(&yaml_object);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "http://localhost:8545");
    }
}
