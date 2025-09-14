use serde::Deserialize;

// In `contender_cli` we can retrieve the SpamCommandArgs struct from this using SpamCommandArgs::from<SpamCommandArgsJsonAdapter>()
#[derive(Deserialize, Debug, Clone)]
pub struct SpamCommandArgsJsonAdapter {
    pub scenario: String,
    pub rpc_url: String,
    pub builder_url: Option<String>,
    pub txs_per_block: Option<u64>,
    pub txs_per_second: Option<u64>,
    pub duration: u64,
    pub private_keys: Option<Vec<String>>,
    pub min_balance: String,
    pub tx_type: String,
    pub timeout_secs: u64,
    pub env: Option<Vec<(String, String)>>,
    pub loops: Option<u64>,
    pub seed: Option<String>,

    pub call_fcu: bool,
    pub use_op: bool,
    pub auth_rpc_url: Option<String>,
    pub jwt_secret: Option<String>,
    pub disable_reporting: bool,
    pub gas_price_percent_add: Option<u64>,
}

// In `contender_cli` we can retrieve the SetupCommandArgs struct from this using SetupCommandArgs::from<SetupCommandArgsJsonAdapter>()
#[derive(Deserialize, Debug, Clone)]
pub struct SetupCommandArgsJsonAdapter {
    pub testfile: String,
    pub rpc_url: String,
    pub private_keys: Option<Vec<String>>,
    pub min_balance: String,
    pub tx_type: String,
    pub env: Option<Vec<(String, String)>>,
    pub call_fcu: bool,
    pub use_op: bool,
    pub auth_rpc_url: Option<String>,
    pub jwt_secret: Option<String>,
    pub seed: Option<String>,
    // pub engine_params: EngineParams,
}
