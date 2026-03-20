use std::sync::Arc;

use axum::{
    extract::{Path, State},
    response::sse::{Event, Sse},
    routing::get,
    Router,
};
use tokio::sync::RwLock;
use tokio_stream::{wrappers::BroadcastStream, StreamExt};
use tracing::warn;

use crate::sessions::ContenderSessionCache;

pub type SharedSessions = Arc<RwLock<ContenderSessionCache>>;

/// Build an axum router that serves SSE log streams.
///
/// `GET /logs/:session_id` — returns an SSE stream of log lines for the given session.
pub fn sse_router(sessions: SharedSessions) -> Router {
    Router::new()
        .route("/logs/{session_id}", get(logs_handler))
        .with_state(sessions)
}

async fn logs_handler(
    Path(session_id): Path<usize>,
    State(sessions): State<SharedSessions>,
) -> Result<
    Sse<impl tokio_stream::Stream<Item = Result<Event, std::convert::Infallible>>>,
    (axum::http::StatusCode, String),
> {
    let sessions = sessions.read().await;
    let session = sessions.get_session(session_id).ok_or_else(|| {
        (
            axum::http::StatusCode::NOT_FOUND,
            format!("Session {session_id} not found"),
        )
    })?;
    let rx = session.log_tx.subscribe();
    drop(sessions);

    let stream = BroadcastStream::new(rx).filter_map(|res| match res {
        Ok(msg) => Some(Ok(Event::default().data(msg))),
        Err(e) => {
            warn!("SSE broadcast lag: {e}");
            None
        }
    });

    Ok(Sse::new(stream))
}
