use alloy::transports::http::reqwest;
use contender_core::{
    error::ContenderError,
    generator::{
        templater::Templater,
        types::{CreateDefinition, FunctionCallDefinition, SpamRequest},
        PlanConfig,
    },
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::read;

/// Configuration to run a test scenario; used to generate PlanConfigs.
/// Defines TOML schema for scenario files.
#[derive(Clone, Deserialize, Debug, Serialize, Default)]
pub struct TestConfig {
    /// Template variables
    pub env: Option<HashMap<String, String>>,

    /// Contract deployments; array of hex-encoded bytecode strings.
    pub create: Option<Vec<CreateDefinition>>,

    /// Setup steps to run before spamming.
    pub setup: Option<Vec<FunctionCallDefinition>>,

    /// Function to call in spam txs.
    pub spam: Option<Vec<SpamRequest>>,
}

impl TestConfig {
    pub async fn from_remote_url(url: &str) -> Result<TestConfig, Box<dyn std::error::Error>> {
        let file_contents = reqwest::get(url)
            .await
            .map_err(|_err| format!("Error occured while fetching URL {url}"))?
            .text()
            .await
            .map_err(|_err| "Cannot convert the contents of the file into text.")?;
        let test_file: TestConfig = toml::from_str(&file_contents)?;
        Ok(test_file)
    }

    pub fn from_file(file_path: &str) -> Result<TestConfig, Box<dyn std::error::Error>> {
        let file_contents_str = String::from_utf8_lossy(&read(file_path)?).to_string();
        let test_file: TestConfig = toml::from_str(&file_contents_str)?;
        Ok(test_file)
    }

    pub fn encode_toml(&self) -> Result<String, Box<dyn std::error::Error>> {
        let encoded = toml::to_string(self)?;
        Ok(encoded)
    }

    pub fn save_toml(&self, file_path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let encoded = self.encode_toml()?;
        std::fs::write(file_path, encoded)?;
        Ok(())
    }
}

impl PlanConfig<String> for TestConfig {
    fn get_spam_steps(&self) -> Result<Vec<SpamRequest>, ContenderError> {
        Ok(self.spam.to_owned().unwrap_or_default())
    }

    fn get_setup_steps(&self) -> Result<Vec<FunctionCallDefinition>, ContenderError> {
        Ok(self.setup.to_owned().unwrap_or_default())
    }

    fn get_create_steps(&self) -> Result<Vec<CreateDefinition>, ContenderError> {
        Ok(self.create.to_owned().unwrap_or_default())
    }

    fn get_env(&self) -> Result<HashMap<String, String>, ContenderError> {
        Ok(self.env.to_owned().unwrap_or_default())
    }
}

impl Templater<String> for TestConfig {
    /// Find values wrapped in brackets in a string and replace them with values from a hashmap whose key match the value in the brackets.
    /// example: "hello {world}" with hashmap {"world": "earth"} will return "hello earth"
    fn replace_placeholders(&self, input: &str, template_map: &HashMap<String, String>) -> String {
        let mut output = input.to_owned();
        for (key, value) in template_map.iter() {
            let template = format!("{{{key}}}");
            output = output.replace(&template, value);
        }
        output
    }

    fn terminator_start(&self, input: &str) -> Option<usize> {
        input.find("{")
    }

    fn terminator_end(&self, input: &str) -> Option<usize> {
        input.find("}")
    }

    fn num_placeholders(&self, input: &str) -> usize {
        input.chars().filter(|&c| c == '{').count()
    }

    fn copy_end(&self, input: &str, last_end: usize) -> String {
        input.split_at(last_end).1.to_owned()
    }

    fn find_key(&self, input: &str) -> Option<(String, usize)> {
        if let Some(template_start) = self.terminator_start(input) {
            let template_end = self.terminator_end(input);
            if let Some(template_end) = template_end {
                let template_name = &input[template_start + 1..template_end];
                return Some((template_name.to_owned(), template_end));
            }
        }
        None
    }
}
