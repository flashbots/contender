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

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::primitives::TxHash;

    fn make_tx(flashblock_index: Option<u64>) -> RunTx {
        RunTx {
            tx_hash: TxHash::ZERO,
            start_timestamp_ms: 1000,
            end_timestamp_ms: Some(2000),
            block_number: Some(1),
            gas_used: Some(21000),
            kind: None,
            error: None,
            flashblock_latency_ms: Some(100),
            flashblock_index,
        }
    }

    #[test]
    fn returns_none_when_no_flashblock_data() {
        let txs = vec![make_tx(None), make_tx(None)];
        assert!(FlashblockIndexChart::new(&txs).is_none());
    }

    #[test]
    fn returns_none_for_empty_input() {
        assert!(FlashblockIndexChart::new(&[]).is_none());
    }

    #[test]
    fn counts_per_index() {
        let txs = vec![
            make_tx(Some(0)),
            make_tx(Some(0)),
            make_tx(Some(1)),
            make_tx(Some(3)),
        ];
        let chart = FlashblockIndexChart::new(&txs).unwrap();
        let data = chart.echart_data();

        assert_eq!(data.labels, vec!["0", "1", "2", "3"]);
        assert_eq!(data.counts, vec![2, 1, 0, 1]);
        assert_eq!(data.max_count, 2);
    }

    #[test]
    fn single_index() {
        let txs = vec![make_tx(Some(0)), make_tx(Some(0))];
        let chart = FlashblockIndexChart::new(&txs).unwrap();
        let data = chart.echart_data();

        assert_eq!(data.labels, vec!["0"]);
        assert_eq!(data.counts, vec![2]);
        assert_eq!(data.max_count, 2);
    }
}
