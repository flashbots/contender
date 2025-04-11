use std::path::PathBuf;

use alloy_rpc_types_engine::JwtSecret;

pub const DEFAULT_BLOCK_TIME: u64 = 1;

pub fn read_jwt_file(jwt_secret_file: &PathBuf) -> Result<JwtSecret, Box<dyn std::error::Error>> {
    if !jwt_secret_file.is_file() {
        return Err(format!(
            "JWT secret file not found: {}",
            jwt_secret_file.to_string_lossy(),
        )
        .into());
    }
    let jwt = std::fs::read_to_string(jwt_secret_file)?;
    Ok(JwtSecret::from_hex(jwt)?)
}
