use contender_core::db::RunTx;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub struct PendingTxsChart {
    /// Maps timestamp (seconds) to number of pending txs
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
        // get min/max timestamps from run_txs (ms), convert to seconds for bucketing
        let (min_timestamp_s, max_timestamp_s) =
            run_txs.iter().fold((u64::MAX, 0), |(min, max), tx| {
                let start_s = tx.start_timestamp_ms / 1000;
                let end_s = tx.end_timestamp_ms.unwrap_or_default() / 1000;
                (min.min(start_s), max.max(end_s))
            });

        // find pending txs for each second, with 1s padding
        for t in min_timestamp_s.saturating_sub(1)..max_timestamp_s + 1 {
            let t_ms = t * 1000;
            let t_ms_end = t_ms + 1000;
            let pending_txs = run_txs
                .iter()
                .filter(|tx| {
                    let start_ms = tx.start_timestamp_ms;
                    let end_ms = tx.end_timestamp_ms.unwrap_or(u64::MAX);
                    start_ms < t_ms_end && t_ms < end_ms
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
