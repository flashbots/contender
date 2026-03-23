use std::sync::Arc;

use axum::{
    extract::{Path, State},
    response::sse::{Event, Sse},
    routing::get,
    Router,
};
use tokio::sync::RwLock;
use tokio_stream::{wrappers::ReceiverStream, Stream};
use tokio_util::sync::CancellationToken;
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
    Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>>,
    (axum::http::StatusCode, String),
> {
    let sessions = sessions.read().await;
    let session = sessions.get_session(session_id).ok_or_else(|| {
        (
            axum::http::StatusCode::NOT_FOUND,
            format!("Session {session_id} not found"),
        )
    })?;
    let rx = session.log_channel.subscribe();
    let cancel = session.cancel.clone();
    drop(sessions);

    let stream = cancel_on_remove(rx, cancel);

    Ok(Sse::new(stream))
}

/// Wraps a broadcast receiver into a stream that terminates when the cancel token fires.
fn cancel_on_remove(
    mut rx: tokio::sync::broadcast::Receiver<String>,
    cancel: CancellationToken,
) -> impl Stream<Item = Result<Event, std::convert::Infallible>> {
    let (tx, mpsc_rx) = tokio::sync::mpsc::channel::<Result<Event, std::convert::Infallible>>(256);
    tokio::spawn(async move {
        loop {
            tokio::select! {
                result = rx.recv() => {
                    match result {
                        Ok(msg) => {
                            if tx.send(Ok(Event::default().data(msg))).await.is_err() {
                                break;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            warn!("SSE broadcast lag: skipped {n} messages");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
                _ = cancel.cancelled() => break,
            }
        }
    });
    ReceiverStream::new(mpsc_rx)
}
