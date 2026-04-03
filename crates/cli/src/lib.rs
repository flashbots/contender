pub mod commands;
pub mod default_scenarios;
pub mod error;
pub mod util;

pub use error::CliError as Error;

// prometheus
use tokio::sync::OnceCell;
pub static PROM: OnceCell<prometheus::Registry> = OnceCell::const_new();
pub static LATENCY_HIST: OnceCell<prometheus::HistogramVec> = OnceCell::const_new();
