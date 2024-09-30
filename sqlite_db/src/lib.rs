use alloy::{
    hex::{FromHex, ToHexExt},
    primitives::{Address, TxHash},
};
use contender_core::db::{DbOps, RunTx};
use contender_core::{error::ContenderError, Result};
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

#[derive(Deserialize, Debug, Serialize)]
struct RunTxRow {
    run_id: i64,
    tx_hash: String,
    start_timestamp: usize,
    end_timestamp: usize,
    block_number: u64,
    gas_used: String,
    kind: String,
}

impl RunTxRow {
    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            run_id: row.get(0)?,
            tx_hash: row.get(1)?,
            start_timestamp: row.get(2)?,
            end_timestamp: row.get(3)?,
            block_number: row.get(4)?,
            gas_used: row.get(5)?,
            kind: row.get(6)?,
        })
    }
}

impl From<RunTxRow> for RunTx {
    fn from(row: RunTxRow) -> Self {
        let tx_hash = TxHash::from_hex(&row.tx_hash).expect("invalid tx hash");
        Self {
            tx_hash,
            start_timestamp: row.start_timestamp,
            end_timestamp: row.end_timestamp,
            block_number: row.block_number,
            gas_used: row.gas_used.parse().expect("invalid gas_used parameter"),
            kind: row.kind,
        }
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
                start_timestamp INTEGER NOT NULL,
                end_timestamp INTEGER NOT NULL,
                block_number INTEGER NOT NULL,
                gas_used TEXT NOT NULL,
                kind TEXT NOT NULL,
                FOREIGN KEY(run_id) REFERENCES runs(runid)
            )",
            params![],
        )?;
        Ok(())
    }

    /// Inserts a new run into the database and returns the ID of the new row.
    fn insert_run(&self, timestamp: u64, tx_count: usize) -> Result<u64> {
        self.execute(
            "INSERT INTO runs (timestamp, tx_count) VALUES (?, ?)",
            params![timestamp, tx_count],
        )?;
        // get ID from newly inserted row
        let id: u64 = self.query_row("SELECT last_insert_rowid()", params![], |row| row.get(0))?;
        Ok(id)
    }

    fn num_runs(&self) -> Result<u64> {
        let count: u64 =
            self.query_row("SELECT COUNT(*) FROM runs", params![], |row| row.get(0))?;
        Ok(count)
    }

    fn get_run_txs(&self, run_id: u64) -> Result<Vec<RunTx>> {
        let pool = self.get_pool()?;
        let mut stmt = pool
            .prepare("SELECT run_id, tx_hash, start_timestamp, end_timestamp, block_number, gas_used, kind FROM run_txs WHERE run_id = ?1")
            .map_err(|e| ContenderError::with_err(e, "failed to prepare statement"))?;

        let rows = stmt
            .query_map(params![run_id], |row| RunTxRow::from_row(row))
            .map_err(|e| ContenderError::with_err(e, "failed to map row"))?;
        let res = rows
            .map(|r| r.map(|r| r.into()))
            .map(|r| r.map_err(|e| ContenderError::with_err(e, "failed to convert row")))
            .collect::<Result<Vec<RunTx>>>()
            .map_err(|e| ContenderError::with_err(e, "failed to collect rows"))?;
        Ok(res)
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

    fn insert_named_txs(&self, named_txs: Vec<(String, TxHash, Option<Address>)>) -> Result<()> {
        let pool = self.get_pool()?;
        let stmts = named_txs.iter().map(|(name, tx_hash, contract_address)| {
            format!(
                "INSERT INTO named_txs (name, tx_hash, contract_address) VALUES ('{}', '{}', '{}');",
                name,
                tx_hash.encode_hex(),
                contract_address.map(|a| a.encode_hex()).unwrap_or_default()
            )
        });
        pool.execute_batch(&format!(
            "BEGIN;
            {}
            COMMIT;",
            stmts
                .reduce(|ac, c| format!("{}\n{}", ac, c))
                .unwrap_or_default(),
        ))
        .map_err(|e| ContenderError::with_err(e, "failed to execute batch"))?;
        Ok(())
    }

    fn get_named_tx(&self, name: &str) -> Result<(TxHash, Option<Address>)> {
        let pool = self.get_pool()?;
        let mut stmt = pool
            .prepare(
                "SELECT name, tx_hash, contract_address FROM named_txs WHERE name = ?1 ORDER BY id DESC LIMIT 1",
            )
            .map_err(|e| ContenderError::with_err(e, "failed to prepare statement"))?;

        let row = stmt
            .query_map(params![name], |row| NamedTxRow::from_row(row))
            .map_err(|e| ContenderError::with_err(e, "failed to map row"))?;
        let res = row
            .last()
            .transpose()
            .map_err(|e| ContenderError::with_err(e, "no row found"))?
            .ok_or(ContenderError::DbError("no existing row", None))?;

        let tx_hash = TxHash::from_hex(&res.tx_hash)
            .map_err(|e| ContenderError::with_err(e, "invalid tx hash"))?;
        let contract_address = res
            .contract_address
            .map(|a| Address::from_hex(&a))
            .transpose()
            .map_err(|e| ContenderError::with_err(e, "invalid address"))?;
        Ok((tx_hash, contract_address))
    }

    fn insert_run_tx(&self, run_id: u64, run_tx: RunTx) -> Result<()> {
        self.execute(
            "INSERT INTO run_txs (run_id, tx_hash, start_timestamp, end_timestamp, block_number, gas_used, kind) VALUES (?, ?, ?, ?, ?, ?, ?)",
            params![
                run_id,
                run_tx.tx_hash.encode_hex(),
                run_tx.start_timestamp,
                run_tx.end_timestamp,
                run_tx.block_number,
                run_tx.gas_used.to_string(),
                run_tx.kind,
            ],
        )
    }

    fn insert_run_txs(&self, run_id: u64, run_txs: Vec<RunTx>) -> Result<()> {
        let pool = self.get_pool()?;
        let stmts = run_txs.iter().map(|tx| {
            format!(
                "INSERT INTO run_txs (run_id, tx_hash, start_timestamp, end_timestamp, block_number, gas_used, kind) VALUES ({}, '{}', {}, {}, {}, '{}', '{}');",
                run_id,
                tx.tx_hash.encode_hex(),
                tx.start_timestamp,
                tx.end_timestamp,
                tx.block_number,
                tx.gas_used.to_string(),
                tx.kind,
            )
        });
        pool.execute_batch(&format!(
            "BEGIN;
            {}
            COMMIT;",
            stmts
                .reduce(|ac, c| format!("{}\n{}", ac, c))
                .unwrap_or_default(),
        ))
        .map_err(|e| ContenderError::with_err(e, "failed to execute batch"))?;
        Ok(())
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

    #[test]
    fn insert_named_txs() {
        let db = SqliteDb::new_memory();
        db.create_tables().unwrap();
        let tx_hash = TxHash::from_slice(&[0u8; 32]);
        let contract_address = Some(Address::from_slice(&[0u8; 20]));
        db.insert_named_txs(vec![
            ("test_tx".to_string(), tx_hash, contract_address),
            ("test_tx2".to_string(), tx_hash, contract_address),
        ])
        .unwrap();
        let count: i64 = db
            .get_pool()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM named_txs", params![], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn inserts_run_txs() {
        let db = SqliteDb::new_memory();
        db.create_tables().unwrap();
        let run_id = db.insert_run(100000, 100).unwrap();
        let run_txs = vec![
            RunTx {
                tx_hash: TxHash::from_slice(&[0u8; 32]),
                start_timestamp: 100,
                end_timestamp: 200,
                block_number: 1,
                gas_used: 100,
                kind: "test".to_string(),
            },
            RunTx {
                tx_hash: TxHash::from_slice(&[1u8; 32]),
                start_timestamp: 200,
                end_timestamp: 300,
                block_number: 2,
                gas_used: 200,
                kind: "test".to_string(),
            },
        ];
        db.insert_run_txs(run_id as u64, run_txs).unwrap();
        let count: i64 = db
            .get_pool()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM run_txs", params![], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);
    }
}
