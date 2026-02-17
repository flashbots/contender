use std::path::{Path, PathBuf};

use super::block_trace::TxTraceReceipt;
use crate::Result;
use alloy::network::AnyRpcBlock;
use serde::{Deserialize, Serialize};

static CACHE_FILENAME: &str = "debug_trace.json";

#[derive(Serialize, Deserialize)]
pub struct CacheFile {
    pub traces: Vec<TxTraceReceipt>,
    pub blocks: Vec<AnyRpcBlock>,
    pub data_dir: PathBuf,
}

impl CacheFile {
    pub fn new(traces: Vec<TxTraceReceipt>, blocks: Vec<AnyRpcBlock>, data_dir: &Path) -> Self {
        Self {
            traces,
            blocks,
            data_dir: data_dir.to_path_buf(),
        }
    }

    pub fn load(data_dir: impl AsRef<Path>) -> Result<Self> {
        let file = std::fs::File::open(cache_path(data_dir.as_ref()))?;
        let cache_data: CacheFile = serde_json::from_reader(file)?;
        Ok(cache_data)
    }

    pub fn save(&self) -> Result<()> {
        let file = std::fs::File::create(cache_path(&self.data_dir))?;
        serde_json::to_writer(file, self)?;
        Ok(())
    }
}

/// Returns the fully-qualified path to the cache file.
fn cache_path(data_dir: &Path) -> PathBuf {
    data_dir.join(CACHE_FILENAME)
}
