use alloy::transports::http::reqwest;
use contender_core::{
    error::ContenderError,
    generator::{
        templater::Templater, types::SpamRequest, CreateDefinition, FunctionCallDefinition,
        PlanConfig,
    },
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, str::FromStr};
use std::{fs::read, ops::Deref};

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
    pub fn new() -> Self {
        TestConfig {
            env: None,
            create: None,
            setup: None,
            spam: None,
        }
    }

    pub async fn from_remote_url(url: &str) -> Result<TestConfig, Box<dyn std::error::Error>> {
        let file_contents = reqwest::get(url)
            .await
            .map_err(|_err| format!("Error occurred while fetching URL {url}"))?
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

    pub fn with_env(mut self, env: HashMap<String, String>) -> Self {
        self.env = Some(env);
        self
    }

    pub fn with_create(mut self, create: Vec<CreateDefinition>) -> Self {
        self.create = Some(create);
        self
    }

    pub fn with_setup(mut self, setup: Vec<FunctionCallDefinition>) -> Self {
        self.setup = Some(setup);
        self
    }

    pub fn with_spam(mut self, spam: Vec<SpamRequest>) -> Self {
        self.spam = Some(spam);
        self
    }
}

impl FromStr for TestConfig {
    type Err = Box<dyn std::error::Error>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let test_file: TestConfig = toml::from_str(s)?;
        Ok(test_file)
    }
}

/// Assigns a given default from_pool if `from_pool` and `from` are `None`.
macro_rules! set_default_from_pool {
    ($fn_call:expr, $from_pool:expr) => {{
        let mut fn_call = $fn_call.to_owned();
        if fn_call.from.is_none() && fn_call.from_pool.is_none() {
            fn_call.from_pool = Some($from_pool.to_owned());
        }
        fn_call
    }};
}

impl PlanConfig<String> for TestConfig {
    fn get_spam_steps(&self) -> Result<Vec<SpamRequest>, ContenderError> {
        use SpamRequest::*;
        let spam_steps: Vec<SpamRequest> = self
            .spam
            .to_owned()
            .unwrap_or_default()
            .iter()
            .map(|step| {
                // process every spam step, including bundle txs
                match step {
                    Tx(fn_call) => Tx(Box::new(set_default_from_pool!(
                        fn_call.deref(),
                        "spammers"
                    ))),
                    Bundle(bundle) => {
                        let mut bundle = bundle.to_owned();
                        let new_txs = bundle
                            .txs
                            .iter()
                            .map(|fn_call| set_default_from_pool!(fn_call, "spammers"))
                            .collect();
                        bundle.txs = new_txs;
                        Bundle(bundle)
                    }
                }
            })
            .collect();
        Ok(spam_steps.to_owned())
    }

    fn get_setup_steps(&self) -> Result<Vec<FunctionCallDefinition>, ContenderError> {
        let setup_steps = self
            .setup
            .to_owned()
            .unwrap_or_default()
            .iter()
            .map(|fn_call| set_default_from_pool!(fn_call, "admin"))
            .collect();
        Ok(setup_steps)
    }

    fn get_create_steps(&self) -> Result<Vec<CreateDefinition>, ContenderError> {
        let create_steps = self
            .create
            .to_owned()
            .unwrap_or_default()
            .iter()
            .map(|fn_call| set_default_from_pool!(fn_call, "admin"))
            .collect();
        Ok(create_steps)
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
