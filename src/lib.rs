pub mod db;
pub mod error;
pub mod spam;

pub type Result<T> = std::result::Result<T, error::ContenderError>;
