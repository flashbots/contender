pub mod block_trace;
pub mod cache;
pub mod chart;
pub mod command;
pub mod error;
pub mod gen_html;
pub mod util;

pub use error::Error;
pub use gen_html::{ChartData, ReportExportV1, ReportMetadata};

pub type Result<T> = std::result::Result<T, Error>;
