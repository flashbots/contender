//! This module provides functionality for estimating quantiles from a set of buckets.
//! It includes a `Bucket` struct representing a single bucket with an upper bound and cumulative count,
//! and a `BucketsExt` trait that provides an extension method for estimating quantiles from a vector of buckets.

#[derive(Debug, Clone)]
pub struct Bucket {
    pub upper_bound: f64,
    pub cumulative_count: u64,
}

impl Bucket {
    fn new(upper_bound: f64, cumulative_count: u64) -> Self {
        Self {
            upper_bound,
            cumulative_count,
        }
    }
}

impl From<(f64, u64)> for Bucket {
    fn from((upper_bound, cumulative_count): (f64, u64)) -> Self {
        Self::new(upper_bound, cumulative_count)
    }
}

pub trait BucketsExt {
    fn estimate_quantile(&self, quantile: f64) -> f64;
}

/// Collects latency metrics from a prometheus registry.
/// Returns a map of RPC method names to latency buckets (upper_bound_secs, cumulative_count).
pub fn collect_latency_from_registry(
    registry: &prometheus::Registry,
) -> std::collections::BTreeMap<String, Vec<Bucket>> {
    use crate::provider::RPC_REQUEST_LATENCY_ID;

    let mut latency_map = std::collections::BTreeMap::new();
    for mf in &registry.gather() {
        if mf.name() != RPC_REQUEST_LATENCY_ID {
            continue;
        }
        for m in mf.get_metric() {
            if m.label.is_empty() {
                continue;
            }
            let label = m.label.first().expect("label");
            if label.name() != "rpc_method" {
                continue;
            }
            let hist = m.get_histogram();
            let buckets: Vec<Bucket> = hist
                .bucket
                .iter()
                .filter_map(|b| Some((b.upper_bound(), b.cumulative_count?).into()))
                .collect();
            latency_map.insert(label.value().to_string(), buckets);
        }
    }
    latency_map
}

impl BucketsExt for Vec<Bucket> {
    fn estimate_quantile(&self, quantile: f64) -> f64 {
        if self.is_empty() {
            return 0.0;
        }

        let total = self.last().expect("empty buckets").cumulative_count;
        let target = (quantile * total as f64).ceil() as u64;

        for i in 0..self.len() {
            if self[i].cumulative_count >= target {
                let lower_bound = if i == 0 { 0.0 } else { self[i - 1].upper_bound };
                let lower_count = if i == 0 {
                    0
                } else {
                    self[i - 1].cumulative_count
                };
                let upper_bound = self[i].upper_bound;
                let upper_count = self[i].cumulative_count;

                let range = (upper_count - lower_count).max(1);
                let position = (target - lower_count) as f64 / range as f64;
                return lower_bound + (upper_bound - lower_bound) * position;
            }
        }

        self.last().unwrap().upper_bound
    }
}
