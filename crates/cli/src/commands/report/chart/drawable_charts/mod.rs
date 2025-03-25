pub mod gas_per_block;
pub mod heatmap;
pub mod pending_txs;
pub mod time_to_inclusion;
mod r#trait;
pub mod tx_gas_used;

pub use r#trait::DrawableChart;
