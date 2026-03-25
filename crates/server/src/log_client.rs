//! Simple test client that subscribes to session logs via JSON-RPC over websocket.
//!
//! Usage:
//!   contender-log-client <session_id> [ws_url]
//!
//! Examples:
//!   contender-log-client 0
//!   contender-log-client 2 ws://127.0.0.1:3000

use jsonrpsee::core::client::SubscriptionClientT;
use jsonrpsee::rpc_params;
use jsonrpsee::ws_client::WsClientBuilder;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let session_id: usize = args
        .get(1)
        .expect("Usage: contender-log-client <session_id> [ws_url]")
        .parse()
        .expect("session_id must be a number");
    let url = args
        .get(2)
        .map(|s| s.as_str())
        .unwrap_or("ws://127.0.0.1:3000");

    eprintln!("Connecting to {url}, subscribing to session {session_id}...");

    let client = WsClientBuilder::default().build(url).await?;

    let mut sub = client
        .subscribe::<String, _>(
            "subscribe_logs",
            rpc_params![session_id],
            "unsubscribe_logs",
        )
        .await?;

    eprintln!("Subscribed. Waiting for logs...\n");

    while let Some(msg) = sub.next().await {
        match msg {
            Ok(line) => println!("{line}"),
            Err(e) => {
                eprintln!("Subscription error: {e}");
                break;
            }
        }
    }

    Ok(())
}
