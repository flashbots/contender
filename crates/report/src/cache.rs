use alloy::network::AnyRpcBlock;
use serde::{Deserialize, Serialize};

use super::block_trace::TxTraceReceipt;

static CACHE_FILENAME: &str = "debug_trace.json";

#[derive(Serialize, Deserialize)]
pub struct CacheFile {
    pub traces: Vec<TxTraceReceipt>,
    pub blocks: Vec<AnyRpcBlock>,
    pub data_dir: String,
}

impl CacheFile {
    pub fn new(traces: Vec<TxTraceReceipt>, blocks: Vec<AnyRpcBlock>, data_dir: &str) -> Self {
        Self {
            traces,
            blocks,
            data_dir: data_dir.to_string(),
        }
    }

    pub fn load(data_dir: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let file = std::fs::File::open(cache_path(data_dir))?;
        let cache_data: CacheFile = serde_json::from_reader(file)?;
        Ok(cache_data)
    }

    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let file = std::fs::File::create(cache_path(&self.data_dir))?;
        serde_json::to_writer(file, self)?;
        Ok(())
    }
}

/// Returns the fully-qualified path to the cache file.
fn cache_path(data_dir: &str) -> String {
    format!("{data_dir}/{CACHE_FILENAME}")
}
