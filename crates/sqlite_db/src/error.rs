use contender_core::db::DbError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("error from db connection pool: {0}")]
    Pool(#[from] r2d2::Error),

    #[error("failed to execute query: {0}")]
    ExecuteQuery(#[from] rusqlite::Error),

    #[error("resource not found: {0}")]
    NotFound(String),
}

impl From<Error> for contender_core::Error {
    fn from(e: Error) -> Self {
        Self::Db(DbError::Internal(e.to_string()))
    }
}

impl From<Error> for contender_core::db::DbError {
    fn from(value: Error) -> Self {
        use Error::*;
        match value {
            Pool(e) => DbError::Internal(format!("db connection pool encountered an error: {e}")),
            ExecuteQuery(e) => DbError::Internal(format!("failed to execute query: {e}")),
            NotFound(e) => DbError::NotFound(e),
        }
    }
}
