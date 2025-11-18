pub mod block_trace;
pub mod cache;
pub mod chart;
pub mod command;
pub mod error;
pub mod gen_html;
pub mod util;

pub use error::Error;

pub type Result<T> = std::result::Result<T, Error>;
