use crate::log_layer::{new_log_sinks, SessionLogRouter, SessionLogSinks};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

pub struct ServerConfig {
    pub rpc_addr: String,
    pub sse_addr: String,
}

/// Load server configuration from environment variables, with defaults.
pub fn load_server_config() -> ServerConfig {
    let rpc_addr = std::env::var("RPC_HOST").unwrap_or("127.0.0.1:3000".to_string());
    let sse_addr = std::env::var("SSE_HOST").unwrap_or("127.0.0.1:3001".to_string());
    ServerConfig { rpc_addr, sse_addr }
}

/// Initialize tracing with a custom layer for routing logs to session-specific sinks.
pub fn init_tracing() -> SessionLogSinks {
    let log_sinks = new_log_sinks();
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_ansi(true)
        .with_target(true)
        .with_line_number(true);

    let session_layer = SessionLogRouter::new(log_sinks.clone());

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt_layer)
        .with(session_layer)
        .init();

    log_sinks
}
