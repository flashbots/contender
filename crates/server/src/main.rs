use std::sync::Arc;

use contender_server::log_layer::{new_log_sinks, SessionLogRouter};
use contender_server::rpc::{ContenderRpcServer as _, ContenderServer};
use contender_server::sessions::ContenderSessionCache;
use contender_server::sse::sse_router;
use jsonrpsee::server::{Server, ServerHandle};
use tokio::sync::RwLock;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

async fn start_rpc_server(
    sessions: Arc<RwLock<ContenderSessionCache>>,
) -> std::io::Result<ServerHandle> {
    let addr = "127.0.0.1:3000";
    let server = Server::builder().build(addr).await?;
    let module = ContenderServer::new(sessions).into_rpc();
    let handle = server.start(module);

    info!("JSON-RPC server listening on {addr}");
    Ok(handle)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let log_sinks = new_log_sinks();

    init_tracing(log_sinks.clone());

    let sessions = Arc::new(RwLock::new(ContenderSessionCache::new(log_sinks)));

    let handle = start_rpc_server(sessions.clone()).await?;

    // SSE endpoint for log streaming (port 3001)
    let sse_app = sse_router(sessions);
    let sse_listener = tokio::net::TcpListener::bind("127.0.0.1:3001").await?;
    info!("SSE server listening on 127.0.0.1:3001");
    let sse_handle = tokio::spawn(async move { axum::serve(sse_listener, sse_app).await });

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

fn init_tracing(log_sinks: contender_server::log_layer::SessionLogSinks) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_ansi(true)
        .with_target(true)
        .with_line_number(true);

    let session_layer = SessionLogRouter::new(log_sinks);

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt_layer)
        .with(session_layer)
        .init();
}
