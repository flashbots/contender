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
    pub spam: Option<Vec<SpamRequest>>, // TODO: figure out how to implement BundleCallDefinition alongside FunctionCallDefinition
}

impl TestConfig {
    async fn fetch_remote_scenario_file_url(
        scenario_path: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        const BASE_URL_SCENARIOS_DIRECTORY: &str =
            "https://raw.githubusercontent.com/flashbots/contender/refs/heads/main/scenarios/";
        let file_url = (String::from(BASE_URL_SCENARIOS_DIRECTORY) + scenario_path).to_owned();
        let file_contents = reqwest::get(file_url).await?.text().await?;
        Ok(file_contents)
    }

    pub async fn from_file(file_path: &str) -> Result<TestConfig, Box<dyn std::error::Error>> {
        let file_contents_str: String;
        if file_path.starts_with("scenario:") {
            file_contents_str =
                Self::fetch_remote_scenario_file_url(&file_path.replace("scenario:", "")).await?;
        } else {
            let file_contents = read(file_path)?;
            file_contents_str = String::from_utf8_lossy(&file_contents).to_string();
        }
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

mod test {

    use super::*;

    #[tokio::test]
    async fn fetch_bad_url() {
        let testconfig = TestConfig::from_file("scenario:bad_path.toml").await;
        assert!(
            testconfig.is_err(),
            "Expected error when fetching non-existent URL"
        );
    }

    #[tokio::test]
    async fn fetch_correct_url_when_prefix_added() {
        let testconfig = TestConfig::from_file("scenario:simpler.toml").await;
        assert!(testconfig.is_ok(), "Can't fetch this URL");
    }

    #[tokio::test]
    async fn dont_fetch_remote_scenario_without_prefix() {
        let testconfig = TestConfig::from_file("bad_prefix:simpler.toml").await;
        assert!(testconfig.is_err(), "URL fetched even without prefix");
    }
}
