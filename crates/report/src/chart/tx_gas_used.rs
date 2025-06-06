use crate::{block_trace::TxTraceReceipt, util::abbreviate_num};
use serde::{Deserialize, Serialize};

pub struct TxGasUsedChart {
    gas_used: Vec<u64>,
    bucket_width: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TxGasUsedData {
    pub buckets: Vec<String>,
    pub counts: Vec<u64>,
    pub max_count: u64,
}

impl TxGasUsedChart {
    pub fn new(trace_data: &[TxTraceReceipt], bucket_width: u64) -> Self {
        let mut gas_used = vec![];
        for t in trace_data {
            gas_used.push(t.receipt.gas_used);
        }
        Self {
            gas_used,
            bucket_width,
        }
    }

    pub fn echart_data(&self) -> TxGasUsedData {
        let mut buckets = vec![];
        let mut counts = vec![];
        let mut max_count = 0;

        for &gas in &self.gas_used {
            let gas = gas + (self.bucket_width - (gas % self.bucket_width));
            let bucket_index = (gas / self.bucket_width) as usize;
            if bucket_index >= buckets.len() {
                buckets.resize(bucket_index + 1, "0".to_string());
                counts.resize(bucket_index + 1, 0);
            }
            counts[bucket_index] += 1;
            if counts[bucket_index] > max_count {
                max_count = counts[bucket_index];
            }
            buckets[bucket_index] = format!(
                "{} - {}",
                abbreviate_num(bucket_index as u64 * self.bucket_width),
                abbreviate_num((bucket_index + 1) as u64 * self.bucket_width)
            );
        }
        let (buckets, counts): (Vec<_>, Vec<_>) = buckets
            .into_iter()
            .zip(counts.into_iter())
            .filter(|(_, count)| *count != 0)
            .unzip();

        TxGasUsedData {
            buckets,
            counts,
            max_count,
        }
    }
}
