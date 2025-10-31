use contender_core::db::RunTx;
use serde::{Deserialize, Serialize};

pub struct TimeToInclusionChart {
    /// Contains each tx's time to inclusion in seconds.
    inclusion_times: Vec<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TimeToInclusionData {
    pub buckets: Vec<String>,
    pub counts: Vec<u64>,
    pub max_count: u64,
}

impl TimeToInclusionChart {
    pub fn new(run_txs: &[RunTx]) -> Self {
        let mut inclusion_times = vec![];
        for tx in run_txs {
            let mut dumb_base = 0;
            if let Some(end_timestamp) = tx.end_timestamp_secs {
                // dumb_base prevents underflow in case system time doesn't match block timestamps
                if dumb_base == 0 && end_timestamp < tx.start_timestamp_secs {
                    dumb_base += tx.start_timestamp_secs - end_timestamp;
                }
                let end_timestamp = end_timestamp + dumb_base;
                let tti = end_timestamp - tx.start_timestamp_secs;
                inclusion_times.push(tti);
            }
        }
        Self { inclusion_times }
    }

    pub fn echart_data(&self) -> TimeToInclusionData {
        let mut buckets = vec![];
        let mut counts = vec![];
        let mut max_count = 0;

        for &tti in &self.inclusion_times {
            let bucket_index = tti as usize; // 1 bucket per second
            if bucket_index >= buckets.len() {
                buckets.resize(bucket_index + 1, "".to_string());
                counts.resize(bucket_index + 1, 0);
            }
            counts[bucket_index] += 1;
            if counts[bucket_index] > max_count {
                max_count = counts[bucket_index];
            }
            buckets[bucket_index] = format!("{bucket_index} - {} s", bucket_index + 1);
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
