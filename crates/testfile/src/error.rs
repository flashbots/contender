use alloy::transports::http::reqwest;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Core(#[from] contender_core::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("reqwest error: {0}")]
    Reqwest(#[from] reqwest::Error),

    #[error("toml deserialization error: {0}")]
    TomlDe(#[from] toml::de::Error),

    #[error("toml serialization error: {0}")]
    TomlSer(#[from] toml::ser::Error),
}
