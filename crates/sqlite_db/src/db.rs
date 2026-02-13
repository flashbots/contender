use crate::{Error, Result};
use alloy::{
    hex::{FromHex, ToHexExt},
    primitives::{Address, FixedBytes, TxHash},
};
use contender_core::{
    buckets::Bucket,
    db::{DbOps, NamedTx, ReplayReport, ReplayReportRequest, RunTx, SpamRun, SpamRunRequest},
};
use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{params, types::FromSql, Row};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::Duration;
use tracing::debug;

// DEV NOTE: increment this const when making changes to the DB schema
use crate::DB_VERSION;

#[derive(Debug)]
struct SqliteConnectionCustomizer;

impl r2d2::CustomizeConnection<rusqlite::Connection, rusqlite::Error>
    for SqliteConnectionCustomizer
{
    fn on_acquire(
        &self,
        conn: &mut rusqlite::Connection,
    ) -> std::result::Result<(), rusqlite::Error> {
        // Enable WAL mode for better concurrent read/write performance.
        // WAL avoids the need to create a rollback journal on every write,
        // preventing SQLITE_CANTOPEN errors under contention.
        conn.execute_batch("PRAGMA journal_mode = WAL;")?;
        // Set a busy timeout so writers retry instead of immediately failing
        // when the database is locked by another connection.
        conn.execute_batch("PRAGMA busy_timeout = 5000;")?;
        Ok(())
    }
}

#[derive(Clone)]
pub struct SqliteDb {
    pool: Pool<SqliteConnectionManager>,
}

impl SqliteDb {
    pub fn from_file(file: &str) -> Result<Self> {
        let manager = SqliteConnectionManager::file(file);
        let pool = Pool::builder()
            .max_size(4)
            .connection_timeout(Duration::from_secs(30))
            .connection_customizer(Box::new(SqliteConnectionCustomizer))
            .build(manager)?;
        Ok(Self { pool })
    }

    pub fn new_memory() -> Self {
        let manager = SqliteConnectionManager::memory();
        let pool = Pool::builder()
            .max_size(1)
            .connection_customizer(Box::new(SqliteConnectionCustomizer))
            .build(manager)
            .expect("failed to create connection pool");
        Self { pool }
    }

    fn get_pool(&self) -> Result<PooledConnection<SqliteConnectionManager>> {
        Ok(self.pool.get()?)
    }

    fn execute<P: rusqlite::Params>(&self, query: &str, params: P) -> Result<()> {
        self.get_pool()?.execute(query, params)?;
        Ok(())
    }

    pub fn table_exists(&self, table_name: &str) -> Result<bool> {
        let exists: bool = self
            .query_row(
                "SELECT name FROM sqlite_master WHERE type='table' AND name=?1",
                params![table_name],
                |_| Ok(true),
            )
            .unwrap_or(false);
        Ok(exists)
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
        tracing::debug!("executing query: {query}");
        Ok(self.get_pool()?.query_row(query, params, with_row)?)
    }
}

#[derive(Deserialize, Debug, Serialize)]
struct NamedTxRow {
    name: String,
    tx_hash: String,
    contract_address: Option<String>,
}

impl From<NamedTxRow> for NamedTx {
    fn from(row: NamedTxRow) -> Self {
        let tx_hash = TxHash::from_hex(&row.tx_hash).expect("invalid tx hash");
        let contract_address = row
            .contract_address
            .map(|a| Address::from_hex(&a))
            .transpose()
            .expect("invalid address");
        NamedTx::new(row.name, tx_hash, contract_address)
    }
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
    start_timestamp: u64,
    end_timestamp: Option<u64>,
    block_number: Option<u64>,
    gas_used: Option<u64>,
    kind: Option<String>,
    error: Option<String>,
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
            error: row.get(7)?,
        })
    }
}

impl From<RunTxRow> for RunTx {
    /// # panics
    /// if the tx_hash is invalid.
    fn from(row: RunTxRow) -> Self {
        let tx_hash = TxHash::from_hex(&row.tx_hash).expect("invalid tx hash");
        Self {
            tx_hash,
            start_timestamp_secs: row.start_timestamp,
            end_timestamp_secs: row.end_timestamp,
            block_number: row.block_number,
            gas_used: row.gas_used,
            kind: row.kind,
            error: row.error,
        }
    }
}

struct SpamRunRow {
    pub id: u64,
    pub timestamp: String,
    pub tx_count: usize,
    pub scenario_name: String,
    pub campaign_id: Option<String>,
    pub campaign_name: Option<String>,
    pub stage_name: Option<String>,
    pub rpc_url: String,
    pub txs_per_duration: u64,
    pub duration: String,
    pub timeout: u64,
}

impl From<SpamRunRow> for SpamRun {
    fn from(row: SpamRunRow) -> Self {
        Self {
            id: row.id,
            timestamp: row.timestamp.parse::<usize>().expect("invalid timestamp"),
            tx_count: row.tx_count,
            scenario_name: row.scenario_name,
            campaign_id: row.campaign_id,
            campaign_name: row.campaign_name,
            stage_name: row.stage_name,
            rpc_url: row.rpc_url,
            txs_per_duration: row.txs_per_duration,
            duration: row.duration.into(),
            timeout: row.timeout,
        }
    }
}

impl DbOps for SqliteDb {
    type Error = Error;

    fn get_rpc_url_id(
        &self,
        rpc_url: impl AsRef<str>,
        genesis_hash: FixedBytes<32>,
    ) -> std::result::Result<u64, Self::Error> {
        let pool = self.get_pool()?;

        // first check the rpc_urls table; insert if not present
        pool.execute(
            "INSERT OR IGNORE INTO rpc_urls (url, genesis_hash) VALUES (?, ?)",
            params![rpc_url.as_ref(), genesis_hash.to_string()],
        )?;

        // then get the rpc_url ID
        let rpc_url_id: i64 = self.query_row(
            &format!(
                "SELECT id FROM rpc_urls WHERE url = '{}' AND genesis_hash = '{}'",
                rpc_url.as_ref(),
                genesis_hash.to_string().to_lowercase()
            ),
            params![],
            |row| row.get(0),
        )?;

        Ok(rpc_url_id as u64)
    }

    fn get_rpc_url_for_scenario(&self, scenario_name: &str) -> Result<Option<String>> {
        let result: Option<String> = self
            .query_row(
                "SELECT rpc_url FROM runs WHERE scenario_name = ?1 ORDER BY id DESC LIMIT 1",
                params![scenario_name],
                |row| row.get(0),
            )
            .ok();
        Ok(result)
    }

    fn version(&self) -> u64 {
        self.query_row("PRAGMA user_version", params![], |row| row.get(0))
            .unwrap_or(0)
    }

    fn create_tables(&self) -> Result<()> {
        let queries = [
            "PRAGMA foreign_keys = ON;",
            &format!("PRAGMA user_version = {DB_VERSION};"),
            "CREATE TABLE runs (
                id INTEGER PRIMARY KEY,
                timestamp TEXT NOT NULL,
                tx_count INTEGER NOT NULL,
                scenario_name TEXT NOT NULL DEFAULT '',
                campaign_id TEXT,
                campaign_name TEXT,
                stage_name TEXT,
                rpc_url TEXT NOT NULL DEFAULT '',
                txs_per_duration INTEGER NOT NULL,
                duration TEXT NOT NULL,
                timeout INTEGER NOT NULL
            )",
            "CREATE TABLE rpc_urls (
                id INTEGER PRIMARY KEY,
                url TEXT NOT NULL,
                genesis_hash TEXT NOT NULL,
                UNIQUE(url, genesis_hash)
            )",
            "CREATE TABLE named_txs (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                tx_hash TEXT NOT NULL,
                contract_address TEXT,
                rpc_url_id INTEGER NOT NULL,
                FOREIGN KEY (rpc_url_id) REFERENCES rpc_urls(id)
            )",
            "CREATE TABLE run_txs (
                id INTEGER PRIMARY KEY,
                run_id INTEGER NOT NULL,
                tx_hash TEXT NOT NULL,
                start_timestamp INTEGER NOT NULL,
                end_timestamp INTEGER,
                block_number INTEGER,
                gas_used INTEGER,
                kind TEXT,
                error TEXT,
                FOREIGN KEY(run_id) REFERENCES runs(id)
            )",
            "CREATE TABLE latency (
                id INTEGER PRIMARY KEY,
                run_id INTEGER NOT NULL,
                upper_bound_secs FLOAT NOT NULL,
                count INTEGER NOT NULL,
                method TEXT NOT NULL,
                FOREIGN KEY(run_id) REFERENCES runs(id)
            )",
            "CREATE TABLE replay_reports (
                id INTEGER PRIMARY KEY,
                rpc_url_id INTEGER NOT NULL,
                gas_per_second INTEGER NOT NULL,
                gas_used INTEGER NOT NULL
            )",
        ];

        for query in queries {
            self.execute(query, params![]).unwrap_or_else(|e| {
                debug!("error from create_tables: {e}");
            });
        }

        Ok(())
    }

    /// Inserts a new run into the database and returns the ID of the new row.
    fn insert_run(&self, run: &SpamRunRequest) -> Result<u64> {
        let SpamRunRequest {
            timestamp,
            tx_count,
            scenario_name,
            campaign_id,
            campaign_name,
            stage_name,
            rpc_url,
            txs_per_duration,
            duration,
            pending_timeout,
        } = run;
        self.execute(
            "INSERT INTO runs (timestamp, tx_count, scenario_name, campaign_id, campaign_name, stage_name, rpc_url, txs_per_duration, duration, timeout) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![timestamp, tx_count, scenario_name, campaign_id, campaign_name, stage_name, rpc_url, txs_per_duration, &duration.to_string(), pending_timeout.as_secs()],
        )?;
        // get ID from newly inserted row
        let id: u64 = self.query_row("SELECT last_insert_rowid()", params![], |row| row.get(0))?;
        Ok(id)
    }

    fn num_runs(&self) -> Result<u64> {
        self.query_row("SELECT COUNT(*) FROM runs", params![], |row| row.get(0))
    }

    fn get_run_txs(&self, run_id: u64) -> Result<Vec<RunTx>> {
        let pool = self.get_pool()?;
        let mut stmt = pool
            .prepare("SELECT run_id, tx_hash, start_timestamp, end_timestamp, block_number, gas_used, kind, error FROM run_txs WHERE run_id = ?1")?;

        let rows = stmt.query_map(params![run_id], RunTxRow::from_row)?;
        let res = rows
            .map(|r| r.map(|r| r.into()))
            .map(|r| r.map_err(|e| e.into()))
            .collect::<Result<Vec<RunTx>>>()?;
        Ok(res)
    }

    fn get_run(&self, run_id: u64) -> Result<Option<SpamRun>> {
        let pool = self.get_pool()?;
        let mut stmt = pool
            .prepare(
                "SELECT id, timestamp, tx_count, scenario_name, campaign_id, campaign_name, stage_name, rpc_url, txs_per_duration, duration, timeout FROM runs WHERE id = ?1",
            )?;

        let row = stmt.query_map(params![run_id], |row| {
            Ok(SpamRunRow {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                tx_count: row.get(2)?,
                scenario_name: row.get(3)?,
                campaign_id: row.get(4)?,
                campaign_name: row.get(5)?,
                stage_name: row.get(6)?,
                rpc_url: row.get(7)?,
                txs_per_duration: row.get(8)?,
                duration: row.get(9)?,
                timeout: row.get(10)?,
            })
        })?;
        let res = row.last().transpose()?;
        Ok(res.map(|r| r.into()))
    }

    fn get_runs_by_campaign(&self, campaign_id: &str) -> Result<Vec<SpamRun>> {
        let pool = self.get_pool()?;
        let mut stmt = pool.prepare(
            "SELECT id, timestamp, tx_count, scenario_name, campaign_id, campaign_name, stage_name, rpc_url, txs_per_duration, duration, timeout FROM runs WHERE campaign_id = ?1 ORDER BY id ASC",
        )?;
        let rows = stmt.query_map(params![campaign_id], |row| {
            Ok(SpamRunRow {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                tx_count: row.get(2)?,
                scenario_name: row.get(3)?,
                campaign_id: row.get(4)?,
                campaign_name: row.get(5)?,
                stage_name: row.get(6)?,
                rpc_url: row.get(7)?,
                txs_per_duration: row.get(8)?,
                duration: row.get(9)?,
                timeout: row.get(10)?,
            })
        })?;
        let res = rows
            .map(|r| r.map(|r| r.into()))
            .map(|r| r.map_err(|e| e.into()))
            .collect::<Result<Vec<SpamRun>>>()?;
        Ok(res)
    }

    fn latest_campaign_id(&self) -> Result<Option<String>> {
        let pool = self.get_pool()?;
        let mut stmt = pool.prepare(
            "SELECT campaign_id FROM runs WHERE campaign_id IS NOT NULL ORDER BY id DESC LIMIT 1",
        )?;
        let row = stmt
            .query_map(params![], |row| row.get(0))?
            .last()
            .transpose()?;
        Ok(row)
    }

    fn insert_named_txs(
        &self,
        named_txs: &[NamedTx],
        rpc_url: &str,
        genesis_hash: FixedBytes<32>,
    ) -> Result<()> {
        let rpc_url_id = self.get_rpc_url_id(rpc_url, genesis_hash)?;

        // Use a transaction for batch inserts with parameterized queries
        let mut conn = self.get_pool()?;
        let tx = conn.transaction()?;

        for named_tx in named_txs {
            tx.execute(
                "INSERT INTO named_txs (name, tx_hash, contract_address, rpc_url_id) VALUES (?1, ?2, ?3, ?4)",
                params![
                    &named_tx.name,
                    named_tx.tx_hash.encode_hex(),
                    named_tx.address.map(|a| a.encode_hex()).unwrap_or_default(),
                    rpc_url_id,
                ],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    fn get_named_tx(
        &self,
        name: &str,
        rpc_url: &str,
        genesis_hash: FixedBytes<32>,
    ) -> Result<Option<NamedTx>> {
        let pool = self.get_pool()?;
        let mut stmt = pool
            .prepare(
                "SELECT name, tx_hash, contract_address, rpc_url_id FROM named_txs WHERE name = ?1 AND rpc_url_id = (
                    SELECT id FROM rpc_urls WHERE url = ?2 AND genesis_hash = ?3
                ) ORDER BY id DESC LIMIT 1",
            )?;

        let row = stmt.query_map(
            params![name, rpc_url, genesis_hash.to_string().to_lowercase()],
            NamedTxRow::from_row,
        )?;
        let res = row.last().transpose()?.map(|r| r.into());
        Ok(res)
    }

    fn get_named_tx_by_address(&self, address: &Address) -> Result<Option<NamedTx>> {
        let pool = self.get_pool()?;
        let mut stmt = pool
            .prepare(
                "SELECT name, tx_hash, contract_address FROM named_txs WHERE contract_address = ?1 ORDER BY id DESC LIMIT 1",
            )?;

        let row = stmt.query_map(params![address.encode_hex()], |row| {
            NamedTxRow::from_row(row)
        })?;
        let res = row.last().transpose()?.map(|r| r.into());
        Ok(res)
    }

    fn get_latency_metrics(&self, run_id: u64, method: &str) -> Result<Vec<Bucket>> {
        let pool = self.get_pool()?;
        let mut stmt = pool.prepare(
            "SELECT upper_bound_secs, count FROM latency WHERE run_id = ?1 AND method = ?2",
        )?;

        let rows = stmt.query_map(params![run_id, method], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?;
        let res = rows
            .map(|r| r.map_err(|e| e.into()))
            .collect::<Result<Vec<(f64, u64)>>>()?
            .into_iter()
            .map(|buckett| buckett.into())
            .collect::<Vec<Bucket>>();
        Ok(res)
    }

    fn insert_run_txs(&self, run_id: u64, run_txs: &[RunTx]) -> Result<()> {
        let mut conn = self.get_pool()?;
        let tx = conn.transaction()?;

        for run_tx in run_txs {
            let val_or_null_u64 =
                |v: &Option<u64>| v.map(|v| v.to_string()).unwrap_or("NULL".to_owned());
            let val_or_null_str = |v: &Option<String>| {
                v.to_owned()
                    .map(|v| format!("'{v}'"))
                    .unwrap_or("NULL".to_owned())
            };

            let kind = val_or_null_str(&run_tx.kind);
            let end_timestamp = val_or_null_u64(&run_tx.end_timestamp_secs);
            let block_number = val_or_null_u64(&run_tx.block_number);
            let gas_used = val_or_null_u64(&run_tx.gas_used);
            let error = val_or_null_str(&run_tx.error);

            tx.execute_batch(&format!(
                "INSERT INTO run_txs (run_id, tx_hash, start_timestamp, end_timestamp, block_number, gas_used, kind, error) VALUES ({}, '{}', {}, {}, {}, {}, {}, {});",
                run_id,
                run_tx.tx_hash.encode_hex(),
                run_tx.start_timestamp_secs,
                end_timestamp,
                block_number,
                gas_used,
                kind,
                error,
            ))?;
        }

        tx.commit()?;
        Ok(())
    }

    fn insert_latency_metrics(
        &self,
        run_id: u64,
        latency_metrics: &BTreeMap<String, Vec<Bucket>>,
    ) -> Result<()> {
        let mut conn = self.get_pool()?;
        let tx = conn.transaction()?;

        for (method, buckets) in latency_metrics {
            for bucket in buckets {
                tx.execute_batch(&format!(
                    "INSERT INTO latency (run_id, upper_bound_secs, count, method) VALUES ({}, {}, {}, '{}');",
                    run_id, bucket.upper_bound, bucket.cumulative_count, method
                ))?;
            }
        }

        tx.commit()?;
        Ok(())
    }

    fn insert_replay_report(&self, report: ReplayReportRequest) -> Result<ReplayReport> {
        let pool = self.get_pool()?;
        let report_id = self.num_replay_reports()? + 1;
        let ReplayReportRequest {
            rpc_url_id,
            gas_per_second,
            gas_used,
        } = report.clone();

        pool.execute(
            "INSERT INTO replay_reports (rpc_url_id, gas_per_second, gas_used) VALUES (?, ?, ?)",
            params![rpc_url_id, gas_per_second, gas_used],
        )?;

        Ok(ReplayReport::new(report_id, report))
    }

    fn get_replay_report(&self, id: u64) -> Result<ReplayReport> {
        let pool = self.get_pool()?;
        let mut stmt = pool.prepare(
            "SELECT id, rpc_url_id, gas_per_second, gas_used FROM replay_reports WHERE id = ?1",
        )?;
        let row = stmt.query_map(params![id], |row| {
            let req = ReplayReportRequest {
                rpc_url_id: row.get(1)?,
                gas_per_second: row.get(2)?,
                gas_used: row.get(3)?,
            };
            Ok(ReplayReport::new(id, req))
        })?;
        let res = row
            .last()
            .transpose()?
            .ok_or(Error::NotFound(format!("replay_reports({id})")))?;
        Ok(res)
    }

    fn num_replay_reports(&self) -> Result<u64> {
        self.query_row("SELECT COUNT(*) FROM replay_reports", params![], |row| {
            row.get(0)
        })
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use alloy::primitives::{FixedBytes, U256};
    use contender_core::db::SpamDuration;

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
        let do_it = || {
            let run = SpamRunRequest {
                timestamp: 100,
                tx_count: 20,
                scenario_name: "test".to_string(),
                campaign_id: None,
                campaign_name: None,
                stage_name: None,
                rpc_url: "http://test:8545".to_string(),
                txs_per_duration: 10,
                duration: SpamDuration::Seconds(10),
                pending_timeout: Duration::from_secs(12),
            };
            db.insert_run(&run).unwrap()
        };

        println!("id: {}", do_it());
        println!("id: {}", do_it());
        println!("id: {}", do_it());
        assert_eq!(db.num_runs().unwrap(), 3);
    }

    #[test]
    fn groups_runs_by_campaign_id() {
        let db = SqliteDb::new_memory();
        db.create_tables().unwrap();
        let pending_timeout = Duration::from_secs(12);
        let mk_run = |scenario: &str| SpamRunRequest {
            timestamp: 100,
            tx_count: 10,
            scenario_name: scenario.to_string(),
            campaign_id: Some("cmp-test".to_string()),
            campaign_name: Some("cmp".to_string()),
            stage_name: Some("stage-a".to_string()),
            rpc_url: "http://test:8545".to_string(),
            txs_per_duration: 5,
            duration: SpamDuration::Seconds(2),
            pending_timeout,
        };

        let first = db.insert_run(&mk_run("scenario:a")).unwrap();
        let second = db.insert_run(&mk_run("scenario:b")).unwrap();
        assert_ne!(first, second);

        let runs = db.get_runs_by_campaign("cmp-test").unwrap();
        assert_eq!(runs.len(), 2);
        assert_ne!(runs[0].id, runs[1].id);
        assert!(runs
            .iter()
            .all(|r| r.campaign_id.as_deref() == Some("cmp-test")));
        assert!(runs
            .iter()
            .all(|r| r.campaign_name.as_deref() == Some("cmp")));
        assert!(runs
            .iter()
            .all(|r| r.stage_name.as_deref() == Some("stage-a")));
        let scenario_names: Vec<_> = runs.iter().map(|r| r.scenario_name.as_str()).collect();
        assert!(scenario_names.contains(&"scenario:a"));
        assert!(scenario_names.contains(&"scenario:b"));
    }

    #[test]
    fn inserts_and_gets_named_txs() {
        let db = SqliteDb::new_memory();
        db.create_tables().unwrap();
        let tx_hash = TxHash::from_slice(&[0u8; 32]);
        let contract_address = Some(Address::from_slice(&[4u8; 20]));
        let name1 = "test_tx".to_string();
        let name2 = "test_tx2";
        let rpc_url = "http://test.url:8545";
        db.insert_named_txs(
            &[
                NamedTx::new(name1.to_owned(), tx_hash, contract_address),
                NamedTx::new(name2.to_string(), tx_hash, contract_address),
            ],
            rpc_url,
            FixedBytes::default(),
        )
        .unwrap();
        let count: i64 = db
            .get_pool()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM named_txs", params![], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 2);

        let res1 = db
            .get_named_tx(&name1, rpc_url, Default::default())
            .unwrap()
            .unwrap();
        assert_eq!(res1.name, name1);
        assert_eq!(res1.tx_hash, tx_hash);
        assert_eq!(res1.address, contract_address);
        let res2 = db
            .get_named_tx(&name1, "http://wrong.url:8545", Default::default())
            .unwrap();
        assert!(res2.is_none());
    }

    #[test]
    fn inserts_and_gets_run_txs() {
        let db = SqliteDb::new_memory();
        db.create_tables().unwrap();
        let run = SpamRunRequest {
            timestamp: 100,
            tx_count: 20,
            scenario_name: "test".to_string(),
            campaign_id: None,
            campaign_name: None,
            stage_name: None,
            rpc_url: "http://test:8545".to_string(),
            txs_per_duration: 10,
            duration: SpamDuration::Seconds(10),
            pending_timeout: Duration::from_secs(12),
        };
        let run_id = db.insert_run(&run).unwrap();
        let run_txs = vec![
            RunTx {
                tx_hash: TxHash::from_slice(&[0u8; 32]),
                start_timestamp_secs: 100,
                end_timestamp_secs: Some(200),
                block_number: Some(1),
                gas_used: Some(100),
                kind: Some("test".to_string()),
                error: None,
            },
            RunTx {
                tx_hash: TxHash::from_slice(&[1u8; 32]),
                start_timestamp_secs: 200,
                end_timestamp_secs: Some(300),
                block_number: Some(2),
                gas_used: Some(200),
                kind: Some("test".to_string()),
                error: None,
            },
        ];
        db.insert_run_txs(run_id, &run_txs).unwrap();
        let count: i64 = db
            .get_pool()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM run_txs", params![], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);

        let res = db.get_run_txs(run_id).unwrap();
        assert_eq!(res.len(), 2);
    }

    #[test]
    fn inserts_and_gets_replay_reports() {
        let db = SqliteDb::new_memory();
        db.create_tables().unwrap();
        let rpc_url_id = db
            .get_rpc_url_id("http://test:8545", FixedBytes::from(U256::from(100)))
            .unwrap();
        println!("rpc_url_id: {rpc_url_id}");
        let req = ReplayReportRequest {
            rpc_url_id,
            gas_per_second: 420000000,
            gas_used: 420000000000,
        };
        println!("req: {req:?}");
        let report = db.insert_replay_report(req.to_owned()).unwrap();
        println!("report: {report:?}");
        let fetched_report = db.get_replay_report(report.id).unwrap();
        assert_eq!(fetched_report.gas_per_second(), req.gas_per_second);
        assert_eq!(fetched_report.gas_used(), req.gas_used);
        assert_eq!(fetched_report.rpc_url_id(), req.rpc_url_id);
    }
}
