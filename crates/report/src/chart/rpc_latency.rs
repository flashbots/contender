use contender_core::buckets::{Bucket, BucketsExt};
use serde::{Deserialize, Serialize};

pub struct LatencyChart {
    buckets: Vec<Bucket>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyQuantiles {
    pub p50: f64,
    pub p90: f64,
    pub p99: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyData {
    pub buckets: Vec<String>,
    pub counts: Vec<u64>,
    pub quantiles: LatencyQuantiles,
}

impl LatencyChart {
    pub fn new(buckets: Vec<Bucket>) -> Self {
        Self { buckets }
    }

    pub fn echart_data(&self) -> LatencyData {
        let buckets: Vec<String> = self
            .buckets
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let upper_ms = b.upper_bound;
                let lower_ms = if i == 0 {
                    0.0
                } else {
                    self.buckets[i - 1].upper_bound
                };

                format!("{} - {}", lower_ms * 1000.0, upper_ms * 1000.0)
            })
            .collect();
        let counts: Vec<u64> = self
            .buckets
            .iter()
            .enumerate()
            .map(|(i, b)| {
                if i == 0 {
                    b.cumulative_count
                } else {
                    // subtract the cumulative count of the previous bucket to get the count for the current bucket
                    // buckets are guaranteed to be monotonically increasing so subtraction underflow isn't a concern
                    b.cumulative_count - self.buckets[i - 1].cumulative_count
                }
            })
            .collect();
        let quantiles = LatencyQuantiles {
            p50: self.buckets.estimate_quantile(0.5) * 1000.0, // convert to ms
            p90: self.buckets.estimate_quantile(0.9) * 1000.0,
            p99: self.buckets.estimate_quantile(0.99) * 1000.0,
        };

        LatencyData {
            buckets,
            counts,
            quantiles,
        }
    }
}
