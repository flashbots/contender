use contender_core::db::RunTx;
use serde::{Deserialize, Serialize};

pub struct FlashblockTimeToInclusionChart {
    /// Contains each tx's flashblock inclusion latency in milliseconds.
    latencies_ms: Vec<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FlashblockTimeToInclusionData {
    pub buckets: Vec<String>,
    pub counts: Vec<u64>,
    pub max_count: u64,
}

impl FlashblockTimeToInclusionChart {
    pub fn new(run_txs: &[RunTx]) -> Option<Self> {
        let latencies_ms: Vec<u64> = run_txs
            .iter()
            .filter_map(|tx| tx.flashblock_latency_ms)
            .collect();

        if latencies_ms.is_empty() {
            return None;
        }

        Some(Self { latencies_ms })
    }

    pub fn echart_data(&self) -> FlashblockTimeToInclusionData {
        let mut buckets = vec![];
        let mut counts = vec![];
        let mut max_count = 0;

        // 100ms per bucket
        let bucket_size_ms = 100;
        for &latency_ms in &self.latencies_ms {
            let bucket_index = (latency_ms / bucket_size_ms) as usize;
            if bucket_index >= buckets.len() {
                buckets.resize(bucket_index + 1, "".to_string());
                counts.resize(bucket_index + 1, 0);
            }
            counts[bucket_index] += 1;
            if counts[bucket_index] > max_count {
                max_count = counts[bucket_index];
            }
            let lo = bucket_index as u64 * bucket_size_ms;
            let hi = lo + bucket_size_ms;
            buckets[bucket_index] = format!("{lo} - {hi} ms");
        }

        // Filter out empty buckets and counts that are zero
        (buckets, counts) = buckets
            .into_iter()
            .zip(counts)
            .filter(|(bucket, count)| !bucket.is_empty() && *count > 0)
            .unzip();

        FlashblockTimeToInclusionData {
            buckets,
            counts,
            max_count,
        }
    }
}
