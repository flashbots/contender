use crate::error::ContenderError;

pub fn spam_rpc(rpc_url: &str, tx_per_second: usize) -> Result<(), ContenderError> {
    // TODO: use MySQL or SQLite to store run data

    println!("Spamming {} with {} tx/s", rpc_url, tx_per_second);

    Ok(())
}
