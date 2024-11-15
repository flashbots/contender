pub mod agent_controller;
pub mod bundle_provider;
pub mod db;
pub mod error;
pub mod generator;
pub mod spammer;
pub mod test_scenario;

pub type Result<T> = std::result::Result<T, error::ContenderError>;
