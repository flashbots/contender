use contender_core::util::TracingOptions;
use contender_server::{ContenderRpcServer as _, ContenderServer};
use jsonrpsee::server::Server;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing();

    let addr = "127.0.0.1:3000";
    let server = Server::builder().build(addr).await?;

    let module = ContenderServer.into_rpc();
    let handle = server.start(module);

    info!("JSON-RPC server listening on {addr}");

    handle.stopped().await;

    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().ok(); // fallback if RUST_LOG is unset
    let mut opts = TracingOptions::default();
    opts = opts.with_line_number(true).with_target(true);
    contender_core::util::init_core_tracing(filter, opts);
}
