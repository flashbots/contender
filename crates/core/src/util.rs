use crate::{generator::types::AnyProvider, Result};
use alloy::{providers::Provider, signers::local::PrivateKeySigner};
use std::str::FromStr;
use tracing::{debug, warn};
use tracing_subscriber::EnvFilter;

/// Derive the block time from the first two blocks after genesis.
pub async fn get_block_time(rpc_client: &AnyProvider) -> Result<u64> {
    // derive block time from first two blocks after genesis.
    // if >2 blocks don't exist, assume block time is 1s
    let block_num = rpc_client.get_block_number().await?;
    let block_time_secs = if block_num >= 2 {
        let mut timestamps = vec![];
        for i in [1_u64, 2] {
            debug!("getting timestamp for block {i}");
            let block = rpc_client.get_block_by_number(i.into()).await?;
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

/// returns blob fee, or 0 if the RPC call fails.
pub async fn get_blob_fee_maybe(rpc_client: &AnyProvider) -> u128 {
    let res = rpc_client.get_blob_base_fee().await;
    if res.is_err() {
        warn!("failed to get blob base fee; defaulting to 0");
    }
    res.unwrap_or(0)
}

#[derive(Debug, Clone)]
pub struct ExtraTxParams {
    pub gas_price: u128,
    pub blob_gas_price: u128,
    pub chain_id: u64,
}

impl From<(u128, u128, u64)> for ExtraTxParams {
    fn from((gas_price, blob_gas_price, chain_id): (u128, u128, u64)) -> Self {
        ExtraTxParams {
            gas_price,
            blob_gas_price,
            chain_id,
        }
    }
}

pub const DEFAULT_PRV_KEYS: [&str; 10] = [
    "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
    "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d",
    "0x5de4111afa1a4b94908f83103eb1f1706367c2e68ca870fc3fb9a804cdab365a",
    "0x7c852118294e51e653712a81e05800f419141751be58f605c371e15141b007a6",
    "0x47e179ec197488593b187f80a00eb0da91f1b9d0b13f8733639f19c30a34926a",
    "0x8b3a350cf5c34c9194ca85829a2df0ec3153be0318b5e2d3348e872092edffba",
    "0x92db14e403b83dfe3df233f83dfa3a0d7096f21ca9b0d6d6b8d88b2b4ec1564e",
    "0x4bbbf85ce3377467afe5d46f804f221813b2bb87f24d81f60f1fcdbf7cbf4356",
    "0xdbda1821b80551c9d65939329250298aa3472ba22feea921c0cf5d620ea67b97",
    "0x2a871d0798f97d79848a013d4936a73bf4cc922c825d33c1cf7073dff6d409c6",
];

pub fn default_signers() -> Vec<PrivateKeySigner> {
    DEFAULT_PRV_KEYS
        .into_iter()
        .map(|k| PrivateKeySigner::from_str(k).expect("Invalid private key"))
        .collect::<Vec<PrivateKeySigner>>()
}

pub fn init_core_tracing(filter: Option<EnvFilter>) {
    let filter = filter.unwrap_or(EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_ansi(true)
        .with_env_filter(filter)
        .with_target(true)
        .with_line_number(true)
        .init();
}
