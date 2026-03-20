pub mod agent_controller;
pub mod buckets;
mod constants;
pub mod db;
pub mod error;
pub mod flashblocks;
pub mod generator;
pub mod orchestrator;
pub mod provider;
pub mod spammer;
pub mod test_scenario;
pub mod util;

pub use error::Error;
pub type Result<T> = std::result::Result<T, error::Error>;

pub use alloy;
pub use contender_bundle_provider::bundle::BundleType;
pub use orchestrator::{Contender, ContenderCtx, RunOpts};
pub use tokio::task as tokio_task;
pub use tokio_util::sync::CancellationToken;

tokio::task_local! {
    /// The session ID for the current task, used by the server's log routing layer
    /// to route tracing events to the correct per-session broadcast channel.
    pub static CURRENT_SESSION_ID: usize;
}

/// Spawn a future that inherits the current `CURRENT_SESSION_ID` task-local (if set).
pub fn spawn_with_session<F>(future: F) -> tokio::task::JoinHandle<F::Output>
where
    F: std::future::Future + Send + 'static,
    F::Output: Send + 'static,
{
    match CURRENT_SESSION_ID.try_with(|id| *id) {
        Ok(id) => tokio::task::spawn(CURRENT_SESSION_ID.scope(id, future)),
        Err(_) => tokio::task::spawn(future),
    }
}
