pub mod agent_controller;
pub mod buckets;
pub mod db;
pub mod error;
pub mod generator;
pub mod provider;
pub mod spammer;
pub mod test_scenario;

pub type Result<T> = std::result::Result<T, error::ContenderError>;
pub use contender_bundle_provider::bundle::BundleType;
