use crate::db::database::DbOps;
use crate::error::ContenderError;
use crate::generator::{
    seeder::{SeedValue, Seeder},
    Generator,
};
use crate::spammer::SpamCallback;
use alloy::hex::{FromHex, ToHexExt};
use alloy::primitives::{Address, TxHash, U256};
use alloy::primitives::{Bytes, TxKind};
use alloy::providers::{Provider, RootProvider};
use alloy::transports::http::{Client, Http};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::read;
use std::sync::Arc;
use tokio::task::{spawn as spawn_task, JoinHandle};

use super::NamedTxRequest;

pub struct ContractDeployer<D>
where
    D: DbOps + Send + Sync + 'static,
{
    config: TestConfig,
    db: Arc<D>,
    rpc_provider: Arc<RootProvider<Http<Client>>>,
}

/// A generator that specifically runs *setup* steps (including contract creation) from a TOML file.
pub struct SetupGenerator<D>
where
    D: DbOps + Send + Sync + 'static,
{
    config: TestConfig,
    db: Arc<D>,
}

/// A generator that specifically runs *spam* steps for TOML files.
/// - `seed` is used to set deterministic sequences of function arguments for the generator
pub struct SpamGenerator<'a, T: Seeder, D>
where
    D: DbOps + Send + Sync + 'static,
{
    config: TestConfig,
    seed: &'a T,
    db: Arc<D>,
}

/// TOML file format.
#[derive(Clone, Deserialize, Debug, Serialize)]
pub struct TestConfig {
    /// Template variables
    pub env: Option<HashMap<String, String>>,

    /// Contract deployments; array of hex-encoded bytecode strings.
    pub create: Option<Vec<CreateDefinition>>,

    /// Setup steps to run before spamming.
    pub setup: Option<Vec<FunctionCallDefinition>>,

    /// Function to call in spam txs.
    pub spam: Option<FunctionCallDefinition>,
}

#[derive(Clone, Deserialize, Debug, Serialize)]
pub struct FunctionCallDefinition {
    /// Address of the contract to call.
    pub to: String,
    /// Address of the tx sender (must be unlocked on RPC endpoint).
    pub from: Option<String>,
    /// Name of the function to call.
    pub signature: String,
    /// Parameters to pass to the function.
    pub args: Vec<String>,
    /// Parameters to fuzz during the test.
    pub fuzz: Option<Vec<FuzzParam>>,
}

#[derive(Clone, Deserialize, Debug, Serialize)]
pub struct CreateDefinition {
    /// Bytecode of the contract to deploy.
    pub bytecode: String,
    /// Name to identify the contract later.
    pub name: String,
    // TODO: support multiple signers
    pub from: Option<String>,
}

#[derive(Clone, Deserialize, Debug, Serialize)]
pub struct FuzzParam {
    /// Name of the parameter to fuzz.
    pub name: String,
    /// Minimum value fuzzer will use.
    pub min: Option<U256>,
    /// Maximum value fuzzer will use.
    pub max: Option<U256>,
}

/// Find values wrapped in brackets in a string and replace them with values from a hashmap whose key match the value in the brackets.
/// example: "hello {world}" with hashmap {"world": "earth"} will return "hello earth"
fn replace_templates(input: &str, template_map: &HashMap<String, String>) -> String {
    let mut output = input.to_owned();
    for (key, value) in template_map.iter() {
        let template = format!("{{{}}}", key);
        output = output.replace(&template, value);
    }
    output
}

impl<'a, T, D> SpamGenerator<'a, T, D>
where
    T: Seeder,
    D: DbOps + Send + Sync + 'static,
{
    pub fn new(config: TestConfig, seed: &'a T, db: D) -> Self {
        Self {
            config,
            seed,
            db: Arc::new(db),
        }
    }
}

impl<D> ContractDeployer<D>
where
    D: DbOps + Send + Sync + 'static,
{
    pub fn new(
        config: TestConfig,
        db: Arc<D>,
        rpc_provider: Arc<RootProvider<Http<Client>>>,
    ) -> Self {
        Self {
            config,
            db,
            rpc_provider,
        }
    }

    pub async fn run(&self) -> Result<(), ContenderError> {
        let mut template_map = HashMap::<String, String>::new();

        // load values from [env] section
        if let Some(env) = &self.config.env {
            for (key, value) in env.iter() {
                template_map.insert(key.to_owned(), value.to_owned());
            }
        }

        if let Some(create_steps) = &self.config.create {
            for step in create_steps.iter() {
                // find & replace templates in bytecode
                find_template_values(&mut template_map, &step.bytecode, self.db.as_ref())?;
                let full_bytecode = replace_templates(&step.bytecode, &template_map);

                let tx = alloy::rpc::types::TransactionRequest {
                    to: Some(TxKind::Create),
                    input: alloy::rpc::types::TransactionInput::both(
                        Bytes::from_hex(&full_bytecode).expect("invalid bytecode hex"),
                    ),
                    ..Default::default()
                };
                let res = self.rpc_provider.send_transaction(tx).await.map_err(|e| {
                    ContenderError::SetupError("failed to send setup tx", Some(e.to_string()))
                })?;
                let receipt = res.get_receipt().await.map_err(|e| {
                    ContenderError::SetupError("failed to get receipt for tx", Some(e.to_string()))
                })?;

                self.db
                    .insert_named_tx(
                        step.name.to_owned(),
                        receipt.transaction_hash,
                        receipt.contract_address,
                    )
                    .expect("failed to insert tx into db");
            }
        }
        Ok(())
    }
}

impl<D> SetupGenerator<D>
where
    D: DbOps + Send + Sync + 'static,
{
    pub fn new(config: TestConfig, db: D) -> Self {
        Self {
            config,
            db: Arc::new(db),
        }
    }
}

impl TestConfig {
    pub fn from_file(file_path: &str) -> Result<TestConfig, Box<dyn std::error::Error>> {
        let file_contents = read(file_path)?;
        let file_contents_str = String::from_utf8_lossy(&file_contents).to_string();
        let test_file: TestConfig = toml::from_str(&file_contents_str)?;
        Ok(test_file)
    }

    pub fn encode_toml(&self) -> Result<String, Box<dyn std::error::Error>> {
        let encoded = toml::to_string(self)?;
        Ok(encoded)
    }

    pub fn save_toml(&self, file_path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let encoded = self.encode_toml()?;
        std::fs::write(file_path, encoded)?;
        Ok(())
    }
}

impl<T, D> Generator for SpamGenerator<'_, T, D>
where
    T: Seeder,
    D: DbOps + Send + Sync + 'static,
{
    fn get_txs(&self, amount: usize) -> Result<Vec<NamedTxRequest>, ContenderError> {
        let mut templates = Vec::new();
        // find all {templates} and fetch their values from the DB
        let mut template_map = HashMap::<String, String>::new();

        // load values from [env] section
        if let Some(env) = &self.config.env {
            for (key, value) in env.iter() {
                template_map.insert(key.to_owned(), value.to_owned());
            }
        }

        if let Some(function) = &self.config.spam {
            let func = alloy_json_abi::Function::parse(&function.signature).map_err(|e| {
                ContenderError::SpamError("failed to parse function name", Some(e.to_string()))
            })?;

            // hashmap to store fuzzy values
            let mut map = HashMap::<String, Vec<U256>>::new();

            // pre-generate fuzzy params
            if let Some(fuzz_params) = function.fuzz.as_ref() {
                // NOTE: This will only generate a single 32-byte value for each fuzzed parameter. Fuzzing values in arrays/structs is not yet supported.
                for fparam in fuzz_params.iter() {
                    let values = self
                        .seed
                        .seed_values(amount, fparam.min, fparam.max)
                        .map(|v| v.as_u256())
                        .collect::<Vec<U256>>();
                    map.insert(fparam.name.to_owned(), values);
                }
            }

            for arg in function.args.iter() {
                find_template_values(&mut template_map, arg, self.db.as_ref())?;
            }
            find_template_values(&mut template_map, &function.to, self.db.as_ref())?;

            // generate spam txs
            for i in 0..amount {
                // encode function arguments
                let mut args = Vec::new();
                for j in 0..function.args.len() {
                    let maybe_fuzz = || {
                        let input_def = func.inputs[j].to_string();
                        // there's probably a better way to do this, but I haven't found it
                        let arg_namedefs =
                            input_def.split_ascii_whitespace().collect::<Vec<&str>>();
                        if arg_namedefs.len() < 2 {
                            // can't fuzz unnamed params
                            return None;
                        }
                        let arg_name = arg_namedefs[1];
                        if map.contains_key(arg_name) {
                            return Some(map.get(arg_name).unwrap()[i].to_string());
                        }
                        None
                    };

                    // !!! args with template values will be overwritten by the fuzzer if it's enabled for this arg
                    let val = maybe_fuzz().unwrap_or_else(|| {
                        let arg = &function.args[j];
                        if arg.contains("{") {
                            replace_templates(arg, &template_map)
                        } else {
                            arg.to_owned()
                        }
                    });
                    args.push(val);
                }

                // replace template value(s) for `to` address
                let to = maybe_replace(&function.to, &template_map);
                let to = to.parse::<Address>().map_err(|e| {
                    ContenderError::SpamError("failed to parse address", Some(e.to_string()))
                })?;

                // input should have all template data filled now
                let input =
                    foundry_common::abi::encode_function_args(&func, args).map_err(|e| {
                        ContenderError::SpamError(
                            "failed to encode function arguments.",
                            Some(e.to_string()),
                        )
                    })?;
                let tx = alloy::rpc::types::TransactionRequest {
                    to: Some(TxKind::Call(to)),
                    input: alloy::rpc::types::TransactionInput::both(input.into()),
                    ..Default::default()
                };
                templates.push(tx.into());
            }
        }

        Ok(templates)
    }
}

fn encode_calldata(args: &[String], sig: &str) -> Result<Vec<u8>, ContenderError> {
    let func = alloy_json_abi::Function::parse(sig).map_err(|e| {
        ContenderError::SpamError("failed to parse setup function name", Some(e.to_string()))
    })?;
    let input = foundry_common::abi::encode_function_args(&func, args).unwrap();
    Ok(input)
}

impl<D> Generator for SetupGenerator<D>
where
    D: DbOps + Send + Sync + 'static,
{
    fn get_txs(&self, _amount: usize) -> crate::Result<Vec<NamedTxRequest>> {
        // let mut contract_deployments = Vec::new();
        let mut tx_templates = Vec::new();
        let mut template_map = HashMap::<String, String>::new();

        // load values from [env] section
        if let Some(env) = &self.config.env {
            for (key, value) in env.iter() {
                template_map.insert(key.to_owned(), value.to_owned());
            }
        }

        if let Some(setup_steps) = &self.config.setup {
            for step in setup_steps.iter() {
                // check `to` field for templates
                find_template_values(&mut template_map, &step.to, self.db.as_ref())?;
                // check all args for templates
                for arg in step.args.iter() {
                    find_template_values(&mut template_map, arg, self.db.as_ref())?;
                }
                // map should be fully populated now with all the template values we need for our txs

                // rebuild args with template values
                let args = step
                    .args
                    .iter()
                    .map(|arg| maybe_replace(arg, &template_map))
                    .collect::<Vec<String>>();

                let input = encode_calldata(&args, &step.signature)?;
                let to = maybe_replace(&step.to, &template_map);
                let to = to.parse::<Address>().map_err(|e| {
                    ContenderError::SpamError("failed to parse address", Some(e.to_string()))
                })?;

                let tx = alloy::rpc::types::TransactionRequest {
                    to: Some(alloy::primitives::TxKind::Call(to)),
                    input: alloy::rpc::types::TransactionInput::both(input.into()),
                    ..Default::default()
                };
                tx_templates.push(tx.into());
            }
        }

        Ok(tx_templates)
    }
}

pub struct NilCallback;

impl NilCallback {
    pub fn new() -> Self {
        Self {}
    }
}

pub type RpcProvider =
    alloy::providers::RootProvider<alloy::transports::http::Http<alloy::transports::http::Client>>;

pub struct SetupCallback<D>
where
    D: DbOps,
{
    pub db: Arc<D>,
    pub rpc_provider: Arc<RpcProvider>,
}

impl<D> SetupCallback<D>
where
    D: DbOps + Send + Sync + 'static,
{
    pub fn new(db: Arc<D>, rpc_provider: Arc<RpcProvider>) -> Self {
        Self { db, rpc_provider }
    }
}

impl SpamCallback for NilCallback {
    fn on_tx_sent(&self, _tx_res: TxHash, _name: Option<String>) -> Option<JoinHandle<()>> {
        // do nothing
        None
    }
}

impl<D> SpamCallback for SetupCallback<D>
where
    D: DbOps + Send + Sync + 'static,
{
    fn on_tx_sent(&self, tx_hash: TxHash, name: Option<String>) -> Option<JoinHandle<()>> {
        let db = self.db.clone();
        let rpc_client = self.rpc_provider.clone();
        let handle = spawn_task(async move {
            // poll for receipt (PendingTransactionBuilder isn't thread-safe but would be nice to use that instead)
            if let Some(name) = name {
                loop {
                    let receipt = rpc_client
                        .get_transaction_receipt(tx_hash)
                        .await
                        .expect(&format!("failed to get receipt for tx {}", tx_hash));
                    if let Some(receipt) = receipt {
                        db.insert_named_tx(name, tx_hash, receipt.contract_address)
                            .expect("failed to insert tx into db");

                        break;
                    }
                    println!("waiting for receipt...");
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                }
            }
        });
        Some(handle)
    }
}

/// Finds all template-value instances in `arg` with values from the DB whose `name` match the {template_name} and saves them into `map`.
fn find_template_values<D: DbOps>(
    map: &mut HashMap<String, String>,
    arg: &str,
    db: &D,
) -> crate::Result<()> {
    // count number of templates (by left brace) in arg
    let num_template_vals = arg.chars().filter(|&c| c == '{').count();
    let mut last_end = 0;

    for _ in 0..num_template_vals {
        let template_value = arg.split_at(last_end).1;
        if let Some(template_start) = template_value.find("{") {
            let template_end = template_value.find("}").ok_or_else(|| {
                ContenderError::SpamError(
                    "failed to find end of template value",
                    Some("missing '}'".to_string()),
                )
            })?;
            let template_name = &template_value[template_start + 1..template_end];
            last_end = template_end + 1;

            // if value not already in map, look up in DB
            if map.contains_key(template_name) {
                continue;
            }

            let template_value = db.get_named_tx(template_name).map_err(|e| {
                ContenderError::SpamError(
                    "failed to get template value from DB",
                    Some(format!("value={} ({})", template_name, e)),
                )
            })?;
            map.insert(
                template_name.to_owned(),
                template_value.1.map(|a| a.encode_hex()).unwrap_or_default(),
            );
        }
    }
    Ok(())
}

fn maybe_replace(arg: &str, template_map: &HashMap<String, String>) -> String {
    if arg.contains("{") {
        replace_templates(arg, &template_map)
    } else {
        arg.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use alloy::providers::ProviderBuilder;
    use alloy::transports::http::reqwest::Url;

    use super::*;
    use crate::db::sqlite::SqliteDb;
    use crate::generator::RandSeed;
    use std::fs;

    fn get_testconfig() -> TestConfig {
        TestConfig {
            env: None,
            create: None,
            setup: None,
            spam: Some(FunctionCallDefinition {
                to: "0x7a250d5630B4cF539739dF2C5dAcb4c659F248DD".to_owned(),
                from: None,
                signature: "swap(uint256 x, uint256 y, address a, bytes b)".to_owned(),
                args: vec![
                    "1".to_owned(),
                    "2".to_owned(),
                    Address::repeat_byte(0x11).encode_hex(),
                    "0xdead".to_owned(),
                ],
                fuzz: None,
            }),
        }
    }

    fn get_fuzzy_testconfig() -> TestConfig {
        TestConfig {
            env: None,
            create: None,
            setup: None,
            spam: Some(FunctionCallDefinition {
                to: "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D".to_owned(),
                from: None,
                signature: "swap(uint256 x, uint256 y, address a, bytes b)".to_owned(),
                args: vec![
                    "1".to_owned(),
                    "2".to_owned(),
                    Address::repeat_byte(0x11).encode_hex(),
                    "0xbeef".to_owned(),
                ],
                fuzz: Some(vec![FuzzParam {
                    name: "x".to_string(),
                    min: None,
                    max: None,
                }]),
            }),
        }
    }

    fn get_setup_testconfig() -> TestConfig {
        TestConfig {
            env: None,
            create: None,
            spam: None,
            setup: Some(vec![
                FunctionCallDefinition {
                    to: "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D".to_owned(),
                    from: None,
                    signature: "swap(uint256 x, uint256 y, address a, bytes b)".to_owned(),
                    args: vec![
                        "1".to_owned(),
                        "2".to_owned(),
                        Address::repeat_byte(0x11).encode_hex(),
                        "0xdead".to_owned(),
                    ],
                    fuzz: None,
                },
                FunctionCallDefinition {
                    to: "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D".to_owned(),
                    from: None,
                    signature: "swap(uint256 x, uint256 y, address a, bytes b)".to_owned(),
                    args: vec![
                        "1".to_owned(),
                        "2".to_owned(),
                        Address::repeat_byte(0x11).encode_hex(),
                        "0xbeef".to_owned(),
                    ],
                    fuzz: None,
                },
            ]),
        }
    }

    const COUNTER_BYTECODE: &'static str =
        "0x608060405234801561001057600080fd5b5060f78061001f6000396000f3fe6080604052348015600f57600080fd5b5060043610603c5760003560e01c80633fb5c1cb1460415780638381f58a146053578063d09de08a14606d575b600080fd5b6051604c3660046083565b600055565b005b605b60005481565b60405190815260200160405180910390f35b6051600080549080607c83609b565b9190505550565b600060208284031215609457600080fd5b5035919050565b60006001820160ba57634e487b7160e01b600052601160045260246000fd5b506001019056fea264697066735822122010f3077836fb83a22ad708a23102f2b487523767e1afef5a93c614619001648b64736f6c63430008170033";

    fn get_create_testconfig() -> TestConfig {
        let mut env = HashMap::new();
        env.insert("test1".to_owned(), "0xbeef".to_owned());
        env.insert("test2".to_owned(), "0x9001".to_owned());
        TestConfig {
            env: Some(env),
            create: Some(vec![CreateDefinition {
                bytecode: COUNTER_BYTECODE.to_string(),
                name: "test_counter".to_string(),
                from: None,
            }]),
            spam: None,
            setup: None,
        }
    }

    fn get_composite_testconfig() -> TestConfig {
        let tc_fuzz = get_fuzzy_testconfig();
        let tc_setup = get_setup_testconfig();
        let tc_create = get_create_testconfig();
        TestConfig {
            env: tc_create.env, // TODO: add something here
            create: tc_create.create,
            spam: tc_fuzz.spam,
            setup: tc_setup.setup,
        }
    }

    #[tokio::test]
    #[ignore = "reason: requires node running on localhost:8545"]
    async fn creates_contracts() -> Result<(), Box<dyn std::error::Error>> {
        let test_file = get_create_testconfig();
        let db = Arc::new(SqliteDb::new_memory());
        db.create_tables().unwrap();
        // TODO: spawn an anvil instance here instead
        let rpc_client = ProviderBuilder::new()
            .on_http(Url::parse("http://localhost:8545").expect("Invalid RPC URL"));
        let gen = ContractDeployer::new(test_file, db.clone(), Arc::new(rpc_client));
        let res = gen.run().await;
        assert!(res.is_ok());
        // read the contract address from the DB
        let contract = db.get_named_tx("test_counter")?;
        assert!(contract.1.is_some());

        Ok(())
    }

    #[test]
    fn parses_testconfig_toml() {
        let test_file = TestConfig::from_file("univ2ConfigTest.toml").unwrap();
        let env = test_file.env.unwrap();
        assert_eq!(
            env.get("feeToSetter").unwrap(),
            "0000000000000000000000000000000000000000"
        );
        assert_eq!(
            test_file.spam.unwrap().from,
            Some("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266".to_owned())
        )
    }

    fn print_testconfig(cfg: &str) {
        println!("{}", "-".repeat(80));
        println!("{}", cfg);
        println!("{}", "-".repeat(80));
    }

    #[test]
    fn encodes_testconfig_toml() {
        let cfg = get_composite_testconfig();
        let encoded = cfg.encode_toml().unwrap();
        print_testconfig(&encoded);
        cfg.save_toml("cargotest.toml").unwrap();
        let test_file2 = TestConfig::from_file("cargotest.toml").unwrap();
        let func = cfg.clone().spam.unwrap();
        assert_eq!(func.to, test_file2.spam.unwrap().to);
        assert_eq!(func.args[0], "1");
        assert_eq!(func.args[1], "2");
        fs::remove_file("cargotest.toml").unwrap();
    }

    #[test]
    fn gets_spam_txs() {
        let test_file = get_testconfig();
        let seed = RandSeed::new();
        let test_gen = SpamGenerator::new(test_file, &seed, SqliteDb::new_memory());
        // this seed can be used to recreate the same test tx(s)
        let spam_txs = test_gen.get_txs(1).unwrap();
        println!("generated test tx(s): {:?}", spam_txs);
        assert_eq!(spam_txs.len(), 1);
        let data = spam_txs[0].tx.input.input.to_owned().unwrap().to_string();
        assert_eq!(data, "0x022c0d9f00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000002000000000000000000000000111111111111111111111111111111111111111100000000000000000000000000000000000000000000000000000000000000800000000000000000000000000000000000000000000000000000000000000002dead000000000000000000000000000000000000000000000000000000000000");
    }

    #[test]
    fn fuzz_is_deterministic() {
        let test_file = get_fuzzy_testconfig();
        let seed = RandSeed::from_bytes(&[0x01; 32]);
        let test_gen = SpamGenerator::new(test_file, &seed, SqliteDb::new_memory());
        let num_txs = 3;
        let spam_txs_1 = test_gen.get_txs(num_txs).unwrap();
        let spam_txs_2 = test_gen.get_txs(num_txs).unwrap();
        for i in 0..num_txs {
            let data1 = spam_txs_1[i].tx.input.input.to_owned().unwrap().to_string();
            let data2 = spam_txs_2[i].tx.input.input.to_owned().unwrap().to_string();
            assert_eq!(data1, data2);
            println!("data1: {}", data1);
            println!("data2: {}", data2);
        }
    }
}
