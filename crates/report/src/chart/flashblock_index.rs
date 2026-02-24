use contender_core::db::RunTx;
use serde::{Deserialize, Serialize};

pub struct FlashblockIndexChart {
    indices: Vec<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FlashblockIndexData {
    pub labels: Vec<String>,
    pub counts: Vec<u64>,
    pub max_count: u64,
}

impl FlashblockIndexChart {
    pub fn new(run_txs: &[RunTx]) -> Option<Self> {
        let indices: Vec<u64> = run_txs
            .iter()
            .filter_map(|tx| tx.flashblock_index)
            .collect();

        if indices.is_empty() {
            return None;
        }

        Some(Self { indices })
    }

    pub fn echart_data(&self) -> FlashblockIndexData {
        let max_index = self.indices.iter().copied().max().unwrap_or(0) as usize;

        let mut counts = vec![0u64; max_index + 1];
        for &idx in &self.indices {
            counts[idx as usize] += 1;
        }

        let labels: Vec<String> = (0..=max_index).map(|i| i.to_string()).collect();
        let max_count = counts.iter().copied().max().unwrap_or(0);

        FlashblockIndexData {
            labels,
            counts,
            max_count,
        }
    }
}
