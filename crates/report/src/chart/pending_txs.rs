use contender_core::db::RunTx;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub struct PendingTxsChart {
    /// Maps timestamp to number of pending txs
    pending_txs_per_second: BTreeMap<u64, u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PendingTxsData {
    pub timestamps: Vec<u64>,
    pub pending_txs: Vec<u64>,
}

impl PendingTxsChart {
    pub fn new(run_txs: &[RunTx]) -> Self {
        let mut pending_txs_per_second = BTreeMap::new();
        // get min/max timestamps from run_txs; evaluate min start_timestamp and max end_timestamp
        let (min_timestamp, max_timestamp) =
            run_txs.iter().fold((u64::MAX, 0), |(min, max), tx| {
                let start_timestamp = tx.start_timestamp_secs;
                let end_timestamp = tx.end_timestamp_secs.unwrap_or_default();
                (min.min(start_timestamp), max.max(end_timestamp))
            });

        // find pending txs for each second, with 1s padding
        for t in min_timestamp - 1..max_timestamp + 1 {
            let pending_txs = run_txs
                .iter()
                .filter(|tx| {
                    let start_timestamp = tx.start_timestamp_secs;
                    let end_timestamp = tx.end_timestamp_secs.unwrap_or(u64::MAX);
                    start_timestamp <= t && t < end_timestamp
                })
                .count() as u64;
            pending_txs_per_second.insert(t, pending_txs);
        }

        Self {
            pending_txs_per_second,
        }
    }

    pub fn echart_data(&self) -> PendingTxsData {
        let mut timestamps = vec![];
        let mut pending_txs = vec![];

        for (timestamp, count) in &self.pending_txs_per_second {
            timestamps.push(*timestamp);
            pending_txs.push(*count);
        }

        PendingTxsData {
            timestamps,
            pending_txs,
        }
    }
}
