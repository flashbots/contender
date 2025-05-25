pub mod agent_controller;
pub mod buckets;
pub mod db;
pub mod error;
pub mod generator;
pub mod provider;
pub mod spammer;
pub mod test_scenario;
pub mod bundle_provider;

pub type Result<T> = std::result::Result<T, error::ContenderError>;
