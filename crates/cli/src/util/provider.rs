//! Contains a wrapper for auth_provider to handle errors in the cli context.

use async_trait::async_trait;
use contender_engine_provider::{
    error::AuthProviderError, AdvanceChain, AuthResult, ChainReplayResults, ControlChain,
    ReplayChain,
};
use tracing::error;

use crate::util::bold;

pub struct AuthClient {
    auth_provider: Box<dyn ControlChain + Send + Sync + 'static>,
}

impl AuthClient {
    pub fn new(auth_provider: Box<dyn ControlChain + Send + Sync + 'static>) -> Self {
        Self { auth_provider }
    }
}

impl ControlChain for AuthClient {}

fn inspect_auth_err(err: &AuthProviderError) {
    use AuthProviderError::*;
    match err {
        MissingBlock(blocknum) => {
            error!("block number {blocknum} was not found onchain")
        }
        InvalidPayload(msg_version, msg) => {
            error!(
                "invalid payload (tried message version {:?}): {}",
                msg_version,
                msg.unwrap_or_default()
            );
            println!("Try changing the message version with {}", bold("-m"));
        }
        InvalidTxs => {
            error!("failed to encode txs in block")
        }
        InvalidBlockRange(start, end) => {
            error!("invalid block range: {start} - {end}")
        }
        InvalidBlockStart(blk) => {
            error!("invalid start block: {blk}")
        }
        InternalError(_, err) => {
            error!("AuthClient encountered an internal error. Please check contender_engine_provider debug logs for more details.");
            let errs = err.to_string();
            if errs.contains("Invalid newPayload") {
                println!(
                    "You may need to specify a different engine message version with {}",
                    bold("--message-version (-m)")
                );
            } else if errs
                .contains("data did not match any variant of untagged enum BlockTransactions")
            {
                println!(
                    "You may need to add the {} flag to target this node.",
                    bold("--op")
                )
            }
        }
        ConnectionFailed(_) => {
            error!("Failed to connect to the auth API. You may need to enable the auth API on your target node.");
        }
        ExtraDataTooShort => {
            error!("Invalid payload.");
            println!(
                "You may need to remove the {} flag to target this node.",
                bold("--op")
            );
        }
        GasLimitRequired => {
            error!("Invalid payload.");
            println!(
                "You may need to pass the {} flag to target this node.",
                bold("--op")
            );
        }
    }
}

#[async_trait]
impl AdvanceChain for AuthClient {
    async fn advance_chain(&self, block_time: u64) -> AuthResult<()> {
        self.auth_provider
            .advance_chain(block_time)
            .await
            .inspect_err(inspect_auth_err)
    }
}

#[async_trait]
impl ReplayChain for AuthClient {
    async fn replay_chain_segment(
        &self,
        start_block: u64,
        end_block: Option<u64>,
    ) -> AuthResult<ChainReplayResults> {
        self.auth_provider
            .replay_chain_segment(start_block, end_block)
            .await
            .inspect_err(inspect_auth_err)
    }
}
