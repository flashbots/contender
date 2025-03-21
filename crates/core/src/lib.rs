pub mod agent_controller;
pub mod db;
pub mod error;
pub mod eth_engine;
pub mod generator;
pub mod provider;
pub mod spammer;
pub mod test_scenario;

pub type Result<T> = std::result::Result<T, error::ContenderError>;
