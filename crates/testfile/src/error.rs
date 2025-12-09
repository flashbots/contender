use alloy::transports::http::reqwest;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("core error")]
    Core(#[from] contender_core::Error),

    #[error("io error")]
    Io(#[from] std::io::Error),

    #[error("reqwest error")]
    Reqwest(#[from] reqwest::Error),

    #[error("toml deserialization error")]
    TomlDe(#[from] toml::de::Error),

    #[error("toml serialization error")]
    TomlSer(#[from] toml::ser::Error),

    #[error("campaign validation error: {0}")]
    Campaign(String),
}
