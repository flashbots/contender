use std::time::Duration;

use alloy::{hex::FromHex, primitives::TxHash};
use futures::StreamExt;
use thiserror::Error;
use tokio::net::TcpStream;
use tokio_tungstenite::{
    tungstenite::{self, Message},
    MaybeTlsStream, WebSocketStream,
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};
use url::Url;

type Result<T> = std::result::Result<T, FlashblocksError>;
type FbStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub struct FlashblocksClient;

impl FlashblocksClient {
    pub async fn connect(
        ws_url: &Url,
    ) -> Result<(
        FbStream,
        tokio_tungstenite::tungstenite::http::Response<Option<Vec<u8>>>,
    )> {
        let (ws_stream, response) = tokio_tungstenite::connect_async(ws_url.as_str())
            .await
            .map_err(|e| FlashblocksError::ConnectionFailed {
                url: ws_url.to_owned(),
                err: e,
            })?;
        Ok((ws_stream, response))
    }

    /// Pre-flight check: connect to flashblocks WS endpoint and validate it serves flashblocks.
    /// The endpoint auto-streams flashblock diffs on connect (no subscription needed).
    /// We verify by waiting for a valid message with `metadata.receipts`.
    pub async fn preflight(ws_url: &Url) -> Result<()> {
        info!("Validating flashblocks WS endpoint: {}", ws_url);

        let (mut ws_stream, _) = Self::connect(ws_url).await?;

        // Wait for a valid flashblock message (with timeout).
        // The endpoint auto-streams — no subscription handshake required.
        // Loop to skip non-data frames (e.g. Ping) until we get a Text/Binary message.
        let timeout_duration = Duration::from_secs(10);
        let preflight_result: String = tokio::time::timeout(timeout_duration, async {
            while let Some(msg_result) = ws_stream.next().await {
                let res = msg_result.map_err(FlashblocksError::PreflightRequestFailed)?;
                if let Some(text) = ws_message_to_text(res) {
                    return Ok(text);
                }
                // Non-data frame (Ping, Pong, etc.) — skip and wait for next
                continue;
            }
            Err(FlashblocksError::PreflightConnectionClosed)
        })
        .await
        .map_err(|_| FlashblocksError::PreflightTimeout(timeout_duration))??;

        let parsed: serde_json::Value = serde_json::from_str(&preflight_result)
            .map_err(|e| FlashblocksError::PreflightInvalidResult(e.to_string()))?;
        if !parsed
            .get("metadata")
            .and_then(|m| m.get("receipts"))
            .ok_or(FlashblocksError::PreflightInvalidResult(preflight_result))?
            .to_string()
            .is_empty()
        {
            info!("Flashblocks WS endpoint validated successfully");
        }

        // Close the preflight connection
        let _ = ws_stream.close(None).await;

        Ok(())
    }

    /// Listens for flashblock diffs over WebSocket and marks matching pending txs.
    /// The endpoint auto-streams flashblock diffs on connect — no subscription needed.
    /// Each message is a JSON object with `metadata.receipts` containing tx hashes as keys.
    pub async fn listen(
        ws_url: &Url,
        fb_sender: tokio::sync::mpsc::Sender<FlashblockMark>,
        cancel_token: CancellationToken,
    ) -> Result<()> {
        info!("Connecting to flashblocks WS: {}", ws_url);
        let (ws_stream, _) = Self::connect(ws_url).await?;
        let (_write, mut read) = ws_stream.split();

        // Process incoming messages — endpoint auto-streams, no subscription needed
        while let Some(msg_result) = read.next().await {
            let msg = msg_result.map_err(|e| {
                cancel_token.cancel();
                FlashblocksError::ConnectionLost(e)
            })?;

            if matches!(msg, Message::Close(_)) {
                cancel_token.cancel();
                return Err(FlashblocksError::ConnectionClosed);
            }

            let text = match ws_message_to_text(msg) {
                Some(t) => t,
                None => continue,
            };

            let timestamp_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("Time went backwards")
                .as_millis();

            // Parse the flashblock diff message
            let parsed: serde_json::Value = match serde_json::from_str(&text) {
                Ok(v) => v,
                Err(_) => continue,
            };

            // Flashblock diff format:
            // {"payload_id":"0x...","index":0,"diff":{"transactions":[...]},"metadata":{"receipts":{"0xTxHash1":{...},...}}}
            // Extract the flashblock index (required field; skip message if absent)
            let index = match parsed.get("index").and_then(|v| v.as_u64()) {
                Some(i) => i,
                None => {
                    warn!("Flashblock diff missing 'index' field, skipping message");
                    continue;
                }
            };

            // Extract tx hashes from metadata.receipts keys
            let receipts_obj = parsed
                .get("metadata")
                .and_then(|m| m.get("receipts"))
                .and_then(|r| r.as_object());

            let tx_hashes: Vec<TxHash> = receipts_obj
                .map(|receipts| {
                    receipts
                        .keys()
                        .filter_map(|k| TxHash::from_hex(k).ok())
                        .collect()
                })
                .unwrap_or_default();

            let has_receipts = receipts_obj.map(|r| r.len()).unwrap_or(0);
            let has_diff_txs = parsed
                .get("diff")
                .and_then(|d| d.get("transactions"))
                .and_then(|t| t.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            debug!(
                "Flashblock diff (index={}): {} metadata.receipts, {} diff.transactions, {} matched tx hashes",
                index, has_receipts, has_diff_txs, tx_hashes.len()
            );

            for tx_hash in tx_hashes {
                if fb_sender
                    .send(FlashblockMark {
                        tx_hash,
                        timestamp_ms,
                        index,
                    })
                    .await
                    .is_err()
                {
                    // Actor shut down
                    info!("Flashblocks listener stopping: actor channel closed");
                    return Ok(());
                }
            }
        }

        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum FlashblocksError {
    #[error("Flashblocks WS connection closed by server.")]
    ConnectionClosed,

    #[error("Failed to connect to flashblocks WS endpoint {url}: {err}")]
    ConnectionFailed {
        err: tokio_tungstenite::tungstenite::Error,
        url: Url,
    },

    #[error("Flashblocks WS connection lost")]
    ConnectionLost(tungstenite::Error),

    #[error("Flashblocks WS connection closed during preflight")]
    PreflightConnectionClosed,

    #[error("Flashblocks WS connection error during preflight")]
    PreflightRequestFailed(tokio_tungstenite::tungstenite::Error),

    #[error("Flashblocks WS endpoint did not send any data within {} seconds", _0.as_secs())]
    PreflightTimeout(std::time::Duration),

    #[error("Flashblocks WS endpoint sent unexpected message format (missing metadata.receipts): {}", &_0[.._0.len().min(200)])]
    PreflightInvalidResult(String),

    #[error("Failed to close write stream for Flashblocks WS endpoint")]
    WriteStreamClose(tokio_tungstenite::tungstenite::Error),
}

/// Flashblock mark from the WS listener (separate channel to avoid backpressure on flush)
pub struct FlashblockMark {
    pub tx_hash: TxHash,
    pub timestamp_ms: u128,
    pub index: u64,
}

/// Extract UTF-8 text from a Text or Binary WebSocket message.
/// Flashblocks endpoints may send JSON as either frame type.
pub fn ws_message_to_text(msg: Message) -> Option<String> {
    match msg {
        Message::Text(t) => Some(t.to_string()),
        Message::Binary(b) => String::from_utf8(b.to_vec()).ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::ws_message_to_text;
    use crate::spammer::tx_actor::PendingRunTx;

    use alloy::primitives::TxHash;
    use tokio_tungstenite::tungstenite::Message;

    fn make_pending_tx(start_ms: u128, fb_ts: Option<u128>, fb_idx: Option<u64>) -> PendingRunTx {
        PendingRunTx {
            tx_hash: TxHash::ZERO,
            start_timestamp_ms: start_ms,
            kind: None,
            error: None,
            flashblock_timestamp_ms: fb_ts,
            flashblock_index: fb_idx,
        }
    }

    #[test]
    fn flashblock_latency_returns_none_when_no_mark() {
        let tx = make_pending_tx(1000, None, None);
        assert_eq!(tx.flashblock_latency_ms(), None);
    }

    #[test]
    fn flashblock_latency_computes_difference() {
        let tx = make_pending_tx(1000, Some(1250), Some(0));
        assert_eq!(tx.flashblock_latency_ms(), Some(250));
    }

    #[test]
    fn flashblock_latency_saturates_on_negative() {
        // If flashblock timestamp is somehow before start (e.g., clock adjustment),
        // saturating_sub should return 0 instead of underflowing.
        let tx = make_pending_tx(2000, Some(1000), Some(0));
        assert_eq!(tx.flashblock_latency_ms(), Some(0));
    }

    #[test]
    fn ws_message_to_text_handles_text_frame() {
        let msg = Message::Text("hello".into());
        assert_eq!(ws_message_to_text(msg), Some("hello".to_string()));
    }

    #[test]
    fn ws_message_to_text_handles_binary_frame() {
        let msg = Message::Binary(b"hello".to_vec().into());
        assert_eq!(ws_message_to_text(msg), Some("hello".to_string()));
    }

    #[test]
    fn ws_message_to_text_returns_none_for_ping() {
        let msg = Message::Ping(vec![].into());
        assert_eq!(ws_message_to_text(msg), None);
    }

    #[test]
    fn ws_message_to_text_returns_none_for_invalid_utf8_binary() {
        let msg = Message::Binary(vec![0xff, 0xfe].into());
        assert_eq!(ws_message_to_text(msg), None);
    }
}
