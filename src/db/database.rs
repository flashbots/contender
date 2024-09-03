use crate::Result;

pub trait DbOps {
    fn new() -> Result<Self>
    where
        Self: Sized;

    fn create_tables(&self) -> Result<()>;

    fn insert_run(&self, timestamp: &str, tx_count: i64, duration: i64) -> Result<()>;

    fn num_runs(&self) -> Result<i64>;
}
