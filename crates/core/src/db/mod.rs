mod error;
mod mock;
mod named_tx;
mod replay_report;
mod runs;
mod spam_duration;
mod r#trait;

pub use error::DbError;
pub use mock::MockDb;
pub use named_tx::*;
pub use r#trait::*;
pub use replay_report::*;
pub use runs::*;
pub use spam_duration::*;

pub type Result<T> = std::result::Result<T, error::DbError>;
