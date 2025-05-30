pub mod agent_controller;
pub mod buckets;
pub mod db;
pub mod error;
pub mod generator;
pub mod provider;
pub mod spammer;
pub mod test_scenario;

pub type Result<T> = std::result::Result<T, error::ContenderError>;

pub use alloy::consensus::TxType;
pub use alloy::primitives as alloy_primitives;
pub use alloy::providers as alloy_providers;
pub use alloy::{signers::local::PrivateKeySigner, transports::http::reqwest::Url};
pub use contender_bundle_provider::bundle::BundleType;
pub use tokio::task as tokio_task;
