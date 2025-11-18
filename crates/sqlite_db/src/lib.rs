mod ctx;
mod db;
pub mod error;

/// Increment this whenever making changes to the DB schema.
pub static DB_VERSION: u64 = 5;

pub use ctx::*;
pub use db::*;
pub use error::Error;

pub type Result<T> = std::result::Result<T, Error>;
