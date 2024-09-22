pub mod db;
pub mod error;
pub mod generator;
pub mod scenario;
pub mod spammer;

pub type Result<T> = std::result::Result<T, error::ContenderError>;
