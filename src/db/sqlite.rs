use super::database::DbOps;
use crate::{error::ContenderError, Result};
use alloy::{
    hex::{FromHex, ToHexExt},
    primitives::{Address, TxHash},
};
use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{params, types::FromSql, Row};
use serde::{Deserialize, Serialize};

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

#[derive(Deserialize, Debug, Serialize)]
struct NamedTxRow {
    name: String,
    tx_hash: String,
    contract_address: Option<String>,
}

impl NamedTxRow {
    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            name: row.get(0)?,
            tx_hash: row.get(1)?,
            contract_address: row.get(2)?,
        })
    }
}

impl DbOps for SqliteDb {
    fn create_tables(&self) -> Result<()> {
        self.execute(
            "CREATE TABLE runs (
                id INTEGER PRIMARY KEY,
                timestamp TEXT NOT NULL,
                tx_count INTEGER NOT NULL
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
        self.execute(
            "CREATE TABLE run_txs (
                id INTEGER PRIMARY KEY,
                run_id INTEGER NOT NULL,
                tx_hash TEXT NOT NULL,
                timestamp INTEGER NOT NULL,
                FOREIGN KEY(run_id) REFERENCES runs(runid)
            )",
            params![],
        )?;
        Ok(())
    }

    fn insert_run(&self, timestamp: u64, tx_count: usize) -> Result<usize> {
        self.execute(
            "INSERT INTO runs (timestamp, tx_count) VALUES (?, ?)",
            params![timestamp, tx_count],
        )?;
        // get ID from newly inserted row
        let id: usize =
            self.query_row("SELECT last_insert_rowid()", params![], |row| row.get(0))?;
        Ok(id)
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

    fn get_named_tx(&self, name: &str) -> Result<(TxHash, Option<Address>)> {
        let pool = self.get_pool()?;
        let mut stmt = pool
            .prepare(
                "SELECT name, tx_hash, contract_address FROM named_txs WHERE name = ?1 ORDER BY id DESC LIMIT 1",
            )
            .map_err(|e| ContenderError::with_err("failed to prepare statement", &e))?;

        let row = stmt
            .query_map(params![name], |row| NamedTxRow::from_row(row))
            .map_err(|e| ContenderError::with_err("failed to map row", &e))?;
        let res = row
            .last()
            .transpose()
            .map_err(|e| ContenderError::with_err("no row found", &e))?
            .ok_or(ContenderError::DbError("no existing row", None))?;

        let tx_hash = TxHash::from_hex(&res.tx_hash)
            .map_err(|e| ContenderError::DbError("invalid tx hash", Some(e.to_string())))?;
        let contract_address = res
            .contract_address
            .map(|a| Address::from_hex(&a))
            .transpose()
            .map_err(|e| ContenderError::DbError("invalid address", Some(e.to_string())))?;
        Ok((tx_hash, contract_address))
    }

    fn insert_run_tx(&self, run_id: i64, tx_hash: TxHash, timestamp: usize) -> Result<()> {
        self.execute(
            "INSERT INTO run_txs (run_id, tx_hash, timestamp) VALUES (?, ?, ?)",
            params![run_id, tx_hash.encode_hex(), timestamp],
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
        let do_it = |num| db.insert_run(100000, num).unwrap();

        println!("id: {}", do_it(100));
        println!("id: {}", do_it(101));
        println!("id: {}", do_it(102));
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
