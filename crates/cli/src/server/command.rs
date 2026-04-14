use std::sync::Arc;

use crate::server::{
    config::{init_tracing, load_server_config},
    rpc_server::{ContenderRpcServer as _, ContenderServer},
    sessions::ContenderSessionCache,
    sse::sse_router,
};
use jsonrpsee::server::{Server, ServerHandle};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tower_http::cors::{Any, CorsLayer};
use tracing::info;

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    // initialize logging w/ a custom layer that pipes logs to session-specific broadcast channels
    let log_sinks = init_tracing();

    // load server config
    let config = load_server_config();

    // shared session cache
    let sessions = Arc::new(RwLock::new(ContenderSessionCache::new(log_sinks)));

    // RPC server for session management and log subscription
    let handle = start_rpc_server(sessions.clone(), &config.rpc_addr).await?;

    // SSE endpoint for log streaming
    let sse_handle = start_sse_server(sessions, &config.sse_addr).await?;

    info!(
        "API docs available at http://{}/openrpc.json",
        config.rpc_addr
    );

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Received Ctrl+C, shutting down...");
        }
        _ = handle.stopped() => {
            info!("RPC server stopped");
        }
        res = sse_handle => {
            info!("SSE server stopped: {:?}", res);
        }
    }

    Ok(())
}

/// Starts a JSON-RPC HTTP server for managing contender sessions,
/// which includes a websocket server for subscribing to session logs.
///
/// Returns a handle to the RPC server; awaiting `.stopped()` on this handle will wait until the server shuts down.
async fn start_rpc_server(
    sessions: Arc<RwLock<ContenderSessionCache>>,
    addr: &str,
) -> std::io::Result<ServerHandle> {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let server = Server::builder()
        .set_http_middleware(tower::ServiceBuilder::new().layer(cors))
        .build(addr)
        .await?;
    let module = ContenderServer::new(sessions).into_rpc();
    let handle = server.start(module);

    info!("JSON-RPC server listening on {addr}");
    Ok(handle)
}

/// Starts the SSE + static file server.
///
/// Serves the web UI at `/`, static assets, and SSE log streams at `/logs/:session_id`.
///
/// Returns a handle to the server task; awaiting this handle will wait until the server shuts down.
async fn start_sse_server(
    sessions: Arc<RwLock<ContenderSessionCache>>,
    addr: &str,
) -> std::io::Result<JoinHandle<std::io::Result<()>>> {
    let sse_app = sse_router(sessions);
    let sse_listener = tokio::net::TcpListener::bind(addr).await?;
    info!("SSE session logs available at http://{addr}/logs/{{session_id}}");
    info!("Web UI available at http://{addr}/");
    let sse_handle = tokio::spawn(async move { axum::serve(sse_listener, sse_app).await });
    Ok(sse_handle)
}
