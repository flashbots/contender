use alloy::rpc::types::Block;
use serde::{Deserialize, Serialize};

use crate::util::data_dir;

use super::block_trace::TxTraceReceipt;

static CACHE_FILENAME: &str = "debug_trace.json";

#[derive(Serialize, Deserialize)]
pub struct CacheFile {
    pub traces: Vec<TxTraceReceipt>,
    pub blocks: Vec<Block>,
}

impl CacheFile {
    pub fn new(traces: Vec<TxTraceReceipt>, blocks: Vec<Block>) -> Self {
        Self { traces, blocks }
    }

    /// Returns the fully-qualified path to the cache file.
    fn cache_path() -> Result<String, Box<dyn std::error::Error>> {
        Ok(format!("{}/{}", data_dir()?, CACHE_FILENAME))
    }

    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let file = std::fs::File::open(CacheFile::cache_path()?)?;
        let cache_data: CacheFile = serde_json::from_reader(file)?;
        Ok(cache_data)
    }

    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let file = std::fs::File::create(CacheFile::cache_path()?)?;
        serde_json::to_writer(file, self)?;
        Ok(())
    }
}
