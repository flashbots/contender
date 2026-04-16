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
pub use orchestrator::{
    Contender, ContenderCtx, Initialized, LifecyclePhase, PhaseMarker, RunOpts, Uninitialized,
};
pub use tokio::task as tokio_task;
pub use tokio_util::sync::CancellationToken;

tokio::task_local! {
    /// The session ID for the current task, used by the server's log routing layer
    /// to route tracing events to the correct per-session broadcast channel.
    pub static CURRENT_SESSION_ID: usize;
}

/// Spawn a future that inherits the current `CURRENT_SESSION_ID` task-local (if set)
/// and instruments it with a `session` tracing span so the fmt layer shows the session ID.
/// If already inside a `session*` span, the existing span is used via `follows_from`.
pub fn spawn_with_session<F>(future: F) -> tokio::task::JoinHandle<F::Output>
where
    F: std::future::Future + Send + 'static,
    F::Output: Send + 'static,
{
    match CURRENT_SESSION_ID.try_with(|id| *id) {
        Ok(id) => {
            let current = tracing::Span::current();
            let has_session_span = current
                .metadata()
                .is_some_and(|m| m.name().starts_with("session"));
            let future = CURRENT_SESSION_ID.scope(id, future);
            if has_session_span {
                tokio::task::spawn(tracing::Instrument::instrument(future, current))
            } else {
                let span = tracing::info_span!("session", id = id);
                tokio::task::spawn(tracing::Instrument::instrument(future, span))
            }
        }
        Err(_) => tokio::task::spawn(future),
    }
}
