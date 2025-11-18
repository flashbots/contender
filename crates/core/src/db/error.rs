use thiserror::Error;

#[derive(Debug, Error)]
pub enum DbError {
    // TODO: add more as we revise sqlite_db
    #[error("db error: {0}")]
    Internal(String),
    #[error("resource not found: {0}")]
    NotFound(String),
}
