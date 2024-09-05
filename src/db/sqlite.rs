use super::database::DbOps;
use crate::{error::ContenderError, Result};
use sqlite::{self, Connection};

pub struct SqliteDb {
    conn: Connection,
}

impl DbOps for SqliteDb {
    fn new() -> Result<Self> {
        let conn = sqlite::open(":memory:")
            .map_err(|_| ContenderError::DbError("failed to open DB", None))?;
        Ok(Self { conn })
    }

    fn create_tables(&self) -> Result<()> {
        self.conn
            .execute(
                "CREATE TABLE runs (
                    id INTEGER PRIMARY KEY,
                    timestamp TEXT NOT NULL,
                    tx_count INTEGER NOT NULL,
                    duration INTEGER NOT NULL
                )",
            )
            .map_err(|_| ContenderError::DbError("failed to create table", None))?;
        Ok(())
    }

    fn insert_run(&self, timestamp: &str, tx_count: i64, duration: i64) -> Result<()> {
        let query = format!(
            "INSERT INTO runs (timestamp, tx_count, duration) VALUES ({}, {}, {})",
            timestamp, tx_count, duration
        );
        self.conn
            .execute(&query)
            .map_err(|_| ContenderError::DbError("failed to insert run", Some(query)))?;
        Ok(())
    }

    fn num_runs(&self) -> Result<i64> {
        let mut stmt = self
            .conn
            .prepare("SELECT COUNT(*) FROM runs")
            .map_err(|_| ContenderError::DbError("failed to prepare query", None))?;
        stmt.next()
            .map_err(|_| ContenderError::DbError("failed to execute query", None))?;
        let count = stmt
            .read::<i64, _>(0)
            .map_err(|_| ContenderError::DbError("failed to read result", None))?;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_table() {
        let db = SqliteDb::new().unwrap();
        db.create_tables().unwrap();
        assert_eq!(db.num_runs().unwrap(), 0);
    }

    #[test]
    fn inserts_runs() {
        let db = SqliteDb::new().unwrap();
        db.create_tables().unwrap();
        db.insert_run("2021-01-01", 100, 10).unwrap();
        db.insert_run("2021-01-01", 101, 10).unwrap();
        db.insert_run("2021-01-01", 102, 10).unwrap();
        assert_eq!(db.num_runs().unwrap(), 3);
    }
}
