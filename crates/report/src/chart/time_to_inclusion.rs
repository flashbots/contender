use contender_core::db::RunTx;
use serde::{Deserialize, Serialize};

pub struct TimeToInclusionChart {
    /// Contains each tx's time to inclusion in milliseconds.
    inclusion_times_ms: Vec<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TimeToInclusionData {
    pub buckets: Vec<String>,
    pub counts: Vec<u64>,
    pub max_count: u64,
}

impl TimeToInclusionChart {
    pub fn new(run_txs: &[RunTx]) -> Self {
        let mut inclusion_times_ms = vec![];
        for tx in run_txs {
            let mut dumb_base = 0;
            if let Some(end_timestamp_ms) = tx.end_timestamp_ms {
                // dumb_base prevents underflow in case system time doesn't match block timestamps
                if dumb_base == 0 && end_timestamp_ms < tx.start_timestamp_ms {
                    dumb_base += tx.start_timestamp_ms - end_timestamp_ms;
                }
                let end_timestamp_ms = end_timestamp_ms + dumb_base;
                let tti_ms = end_timestamp_ms - tx.start_timestamp_ms;
                inclusion_times_ms.push(tti_ms);
            }
        }
        Self { inclusion_times_ms }
    }

    pub fn echart_data(&self) -> TimeToInclusionData {
        let mut buckets = vec![];
        let mut counts = vec![];
        let mut max_count = 0;

        // 1000ms (1s) per bucket
        for &tti_ms in &self.inclusion_times_ms {
            let bucket_index = (tti_ms / 1000) as usize;
            if bucket_index >= buckets.len() {
                buckets.resize(bucket_index + 1, "".to_string());
                counts.resize(bucket_index + 1, 0);
            }
            counts[bucket_index] += 1;
            if counts[bucket_index] > max_count {
                max_count = counts[bucket_index];
            }
            buckets[bucket_index] = format!("{} - {} s", bucket_index, bucket_index + 1);
        }

        // Filter out empty buckets and counts that are zero
        (buckets, counts) = buckets
            .into_iter()
            .zip(counts)
            .filter(|(bucket, count)| !bucket.is_empty() && *count > 0)
            .unzip();

        TimeToInclusionData {
            buckets,
            counts,
            max_count,
        }
    }
}
