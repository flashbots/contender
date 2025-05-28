use hashlink::LinkedHashMap;
use std::error::Error;
use yaml_rust2::Yaml;

pub fn get_testfile(yaml_object: &LinkedHashMap<Yaml, Yaml>) -> Result<String, Box<dyn Error>> {
    match yaml_object.get(&Yaml::String("testfile".into())) {
        Some(testfile_path_value) => match testfile_path_value.as_str() {
            Some(value) => Ok(value.to_owned()),
            None => Err("invalid type of value for 'scenario'".into()),
        },
        None => Err("'scenario' missing in the spam configuration".into()),
    }
}

#[cfg(test)]
mod get_testfile_tests {
    use super::*;

    #[test]
    fn test_valid_testfile_path() {
        let mut yaml = LinkedHashMap::new();
        yaml.insert(
            Yaml::String("testfile".into()),
            Yaml::String("./testfile.toml".into()),
        );

        let result = get_testfile(&yaml);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "./testfile.toml");
    }

    #[test]
    fn test_invalid_type_for_testfile() {
        let mut yaml = LinkedHashMap::new();
        yaml.insert(Yaml::String("testfile".into()), Yaml::Integer(42));

        let result = get_testfile(&yaml);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "invalid type of value for 'scenario'"
        );
    }

    #[test]
    fn test_missing_testfile_key() {
        let yaml = LinkedHashMap::new();

        let result = get_testfile(&yaml);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "'scenario' missing in the spam configuration"
        );
    }
}
