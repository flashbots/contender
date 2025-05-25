pub mod agent_controller;
pub mod buckets;
pub mod bundle_provider;
pub mod db;
pub mod engine_provider;
pub mod error;
pub mod generator;
pub mod provider;
pub mod spammer;
pub mod test_scenario;

pub type Result<T> = std::result::Result<T, error::ContenderError>;
