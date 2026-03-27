use contender_core::db::RunTx;
use serde::{Deserialize, Serialize};

pub struct TimeToInclusionChart {
    /// Contains each tx's time to inclusion in milliseconds.
    inclusion_times_ms: Vec<u64>,
    /// Bucket size in milliseconds for the histogram.
    bucket_size_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TimeToInclusionData {
    pub buckets: Vec<String>,
    pub counts: Vec<u64>,
    pub max_count: u64,
}

impl TimeToInclusionChart {
    pub fn new(run_txs: &[RunTx], bucket_size_ms: u64) -> Self {
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
        Self {
            inclusion_times_ms,
            bucket_size_ms,
        }
    }

    pub fn echart_data(&self) -> TimeToInclusionData {
        let mut buckets = vec![];
        let mut counts = vec![];
        let mut max_count = 0;

        let bucket_size_ms = self.bucket_size_ms;
        for &tti_ms in &self.inclusion_times_ms {
            let bucket_index = (tti_ms / bucket_size_ms) as usize;
            if bucket_index >= buckets.len() {
                buckets.resize(bucket_index + 1, "".to_string());
                counts.resize(bucket_index + 1, 0);
            }
            counts[bucket_index] += 1;
            if counts[bucket_index] > max_count {
                max_count = counts[bucket_index];
            }
            let start_ms = bucket_index as u64 * bucket_size_ms;
            let end_ms = start_ms + bucket_size_ms;
            buckets[bucket_index] = if bucket_size_ms % 1000 == 0 {
                let start_s = start_ms / 1000;
                let end_s = end_ms / 1000;
                format!("{start_s} - {end_s} s")
            } else {
                format!("{start_ms} - {end_ms} ms")
            };
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

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::primitives::TxHash;

    fn make_tx(start_ms: u64, end_ms: u64) -> RunTx {
        RunTx {
            tx_hash: TxHash::ZERO,
            start_timestamp_ms: start_ms,
            end_timestamp_ms: Some(end_ms),
            block_number: Some(1),
            gas_used: Some(21000),
            kind: None,
            error: None,
            flashblock_latency_ms: None,
            flashblock_index: None,
        }
    }

    #[test]
    fn ms_buckets() {
        // 50ms and 80ms fall in bucket 0 (0-100ms), 150ms in bucket 1
        let txs = vec![make_tx(0, 50), make_tx(0, 80), make_tx(0, 150)];
        let data = TimeToInclusionChart::new(&txs, 100).echart_data();

        assert_eq!(data.buckets, vec!["0 - 100 ms", "100 - 200 ms"]);
        assert_eq!(data.counts, vec![2, 1]);
        assert_eq!(data.max_count, 2);
    }

    #[test]
    fn second_buckets() {
        // bucket size is a multiple of 1000ms, so labels should be in seconds
        let txs = vec![make_tx(0, 500), make_tx(0, 1500)];
        let data = TimeToInclusionChart::new(&txs, 1000).echart_data();

        assert_eq!(data.buckets, vec!["0 - 1 s", "1 - 2 s"]);
        assert_eq!(data.counts, vec![1, 1]);
    }

    #[test]
    fn empty_input() {
        let data = TimeToInclusionChart::new(&[], 1000).echart_data();

        assert!(data.buckets.is_empty());
        assert!(data.counts.is_empty());
        assert_eq!(data.max_count, 0);
    }
}
