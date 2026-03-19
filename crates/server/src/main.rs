use contender_core::util::TracingOptions;
use contender_server::rpc::{ContenderRpcServer as _, ContenderServer};
// use contender_server::sessions::ContenderSessionCache;
use jsonrpsee::server::{Server, ServerHandle};
use tracing::info;
use tracing_subscriber::EnvFilter;

async fn start_rpc_server() -> std::io::Result<ServerHandle> {
    let addr = "127.0.0.1:3000";
    let server = Server::builder().build(addr).await?;
    let module = ContenderServer.into_rpc();
    let handle = server.start(module);

    info!("JSON-RPC server listening on {addr}");
    Ok(handle)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing();

    // let mut sessions = ContenderSessionCache::new();

    let handle = start_rpc_server().await?;
    handle.stopped().await;

    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().ok(); // fallback if RUST_LOG is unset
    let mut opts = TracingOptions::default();
    opts = opts.with_line_number(true).with_target(true);
    contender_core::util::init_core_tracing(filter, opts);
}
