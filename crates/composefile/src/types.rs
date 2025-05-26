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
    // TODO: These params are hardcoded for now, I'll need some more info in these before implementing them

    // pub engine_params: EngineParamsAdat,
    // pub gas_price_percent_add: Option<u64>,
    // pub seed: String,
    // pub disable_reporting: bool,
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
    // TODO: These params are hardcoded for now, I'll need some more info in these before implementing them

    // pub seed: RandSeed,
    // pub engine_params: EngineParams,
}
