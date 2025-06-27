use crate::{error::ContenderError, generator::types::AnyProvider, Result};
use alloy::providers::Provider;
use tracing::debug;

/// Derive the block time from the first two blocks after genesis.
pub async fn get_block_time(rpc_client: &AnyProvider) -> Result<u64> {
    // derive block time from first two blocks after genesis.
    // if >2 blocks don't exist, assume block time is 1s
    let block_num = rpc_client
        .get_block_number()
        .await
        .map_err(|e| ContenderError::with_err(e, "failed to get block number"))?;
    let block_time_secs = if block_num >= 2 {
        let mut timestamps = vec![];
        for i in [1_u64, 2] {
            debug!("getting timestamp for block {i}");
            let block = rpc_client
                .get_block_by_number(i.into())
                .await
                .map_err(|e| ContenderError::with_err(e, "failed to get block"))?;
            if let Some(block) = block {
                timestamps.push(block.header.timestamp);
            }
        }
        if timestamps.len() == 2 {
            (timestamps[1] - timestamps[0]).max(1)
        } else {
            1
        }
    } else {
        1
    };
    Ok(block_time_secs)
}
