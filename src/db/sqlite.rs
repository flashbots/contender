use super::database::DbOps;
use crate::{error::ContenderError, Result};
use alloy::{
    hex::ToHexExt,
    primitives::{Address, TxHash},
};
use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{params, types::FromSql, Row};

#[derive(Clone)]
pub struct SqliteDb {
    pool: Pool<SqliteConnectionManager>,
}

impl SqliteDb {
    pub fn from_file(file: &str) -> Result<Self> {
        let manager = SqliteConnectionManager::file(file);
        let pool = Pool::new(manager).map_err(|e| {
            ContenderError::DbError("failed to create connection pool", Some(e.to_string()))
        })?;
        Ok(Self { pool })
    }

    pub fn new_memory() -> Self {
        let manager = SqliteConnectionManager::memory();
        let pool = Pool::new(manager).expect("failed to create connection pool");
        Self { pool }
    }

    fn get_pool(&self) -> Result<PooledConnection<SqliteConnectionManager>> {
        self.pool.get().map_err(|e| {
            ContenderError::DbError("failed to get connection from pool", Some(e.to_string()))
        })
    }

    fn execute<P: rusqlite::Params>(&self, query: &str, params: P) -> Result<()> {
        self.get_pool()?
            .execute(query, params)
            .map_err(|e| ContenderError::DbError("failed to execute query", Some(e.to_string())))?;
        Ok(())
    }

    fn query_row<
        T: FromSql,
        P: rusqlite::Params,
        F: FnOnce(&Row<'_>) -> std::result::Result<T, rusqlite::Error>,
    >(
        &self,
        query: &str,
        params: P,
        with_row: F,
    ) -> Result<T> {
        self.get_pool()?
            .query_row(query, params, with_row)
            .map_err(|e| ContenderError::DbError("failed to query row", Some(e.to_string())))
    }
}

impl DbOps for SqliteDb {
    fn create_tables(&self) -> Result<()> {
        self.execute(
            "CREATE TABLE runs (
                id INTEGER PRIMARY KEY,
                timestamp TEXT NOT NULL,
                tx_count INTEGER NOT NULL,
                duration INTEGER NOT NULL
            )",
            params![],
        )?;
        self.execute(
            "CREATE TABLE named_txs (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                tx_hash TEXT NOT NULL,
                contract_address TEXT
            )",
            params![],
        )?;
        Ok(())
    }

    fn insert_run(&self, timestamp: &str, tx_count: i64, duration: i64) -> Result<()> {
        self.execute(
            "INSERT INTO runs (timestamp, tx_count, duration) VALUES (?, ?, ?)",
            params![timestamp, tx_count, duration],
        )
    }

    fn num_runs(&self) -> Result<i64> {
        let count: i64 =
            self.query_row("SELECT COUNT(*) FROM runs", params![], |row| row.get(0))?;
        Ok(count)
    }

    fn insert_named_tx(
        &self,
        name: String,
        tx_hash: TxHash,
        contract_address: Option<Address>,
    ) -> Result<()> {
        self.execute(
            "INSERT INTO named_txs (name, tx_hash, contract_address) VALUES (?, ?, ?)",
            params![
                name,
                tx_hash.encode_hex(),
                contract_address.map(|a| a.encode_hex())
            ],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_table() {
        let db = SqliteDb::new_memory();
        db.create_tables().unwrap();
        assert_eq!(db.num_runs().unwrap(), 0);
    }

    #[test]
    fn inserts_runs() {
        let db = SqliteDb::new_memory();
        db.create_tables().unwrap();
        db.insert_run("2021-01-01", 100, 10).unwrap();
        db.insert_run("2021-01-01", 101, 10).unwrap();
        db.insert_run("2021-01-01", 102, 10).unwrap();
        assert_eq!(db.num_runs().unwrap(), 3);
    }

    #[test]
    fn inserts_named_tx() {
        let db = SqliteDb::new_memory();
        db.create_tables().unwrap();
        let tx_hash = TxHash::from_slice(&[0u8; 32]);
        let contract_address = Some(Address::from_slice(&[0u8; 20]));
        db.insert_named_tx("test_tx".to_string(), tx_hash, contract_address)
            .unwrap();
        let count: i64 = db
            .get_pool()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM named_txs", params![], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 1);
    }
}
