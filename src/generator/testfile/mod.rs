use crate::db::database::DbOps;
use crate::error::ContenderError;
use crate::generator::{
    seeder::{SeedValue, Seeder},
    templater::Templater,
    types::{CreateDefinition, FunctionCallDefinition, NamedTxRequest, RpcProvider, TestConfig},
    util::encode_calldata,
    Generator, PlanConfig,
};
use crate::spammer::OnTxSent;
use alloy::hex::ToHexExt;
use alloy::primitives::TxKind;
use alloy::primitives::{Address, TxHash, U256};
use std::collections::HashMap;
use std::fs::read;
use std::sync::Arc;
use tokio::task::{spawn as spawn_task, JoinHandle};

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

impl PlanConfig<String> for TestConfig {
    fn get_spam_steps(&self) -> Result<Vec<FunctionCallDefinition>, ContenderError> {
        self.spam
            .to_owned()
            .ok_or(ContenderError::SpamError("no spam steps found", None))
    }

    fn get_setup_steps(&self) -> Result<Vec<FunctionCallDefinition>, ContenderError> {
        self.setup
            .to_owned()
            .ok_or(ContenderError::SetupError("no setup steps found", None))
    }

    fn get_create_steps(&self) -> Result<Vec<CreateDefinition>, ContenderError> {
        self.create
            .to_owned()
            .ok_or(ContenderError::SetupError("no create steps found", None))
    }

    fn get_env(&self) -> Result<HashMap<String, String>, ContenderError> {
        self.env.to_owned().ok_or(ContenderError::SetupError(
            "no environment variables found",
            None,
        ))
    }
}

impl Templater<String> for TestConfig {
    fn replace_placeholders(&self, input: &str, template_map: &HashMap<String, String>) -> String {
        replace_templates(&input, template_map)
    }

    fn terminator_start(&self, input: &str) -> Option<usize> {
        input.find("{")
    }

    fn terminator_end(&self, input: &str) -> Option<usize> {
        input.find("}")
    }

    fn num_placeholders(&self, input: &str) -> usize {
        input.chars().filter(|&c| c == '{').count()
    }

    fn copy_end(&self, input: &str, last_end: usize) -> String {
        input.split_at(last_end).1.to_owned()
    }

    fn find_key(&self, input: &str) -> Option<(String, usize)> {
        if let Some(template_start) = self.terminator_start(input) {
            let template_end = self.terminator_end(input);
            if let Some(template_end) = template_end {
                let template_name = &input[template_start + 1..template_end];
                return Some((template_name.to_owned(), template_end));
            }
        }
        None
    }

    fn encode_contract_address(&self, input: &Address) -> String {
        input.encode_hex()
    }
}

impl<T, D> Generator for SpamGenerator<'_, T, D>
where
    T: Seeder,
    D: DbOps + Send + Sync + 'static,
{
    fn get_txs(&self, amount: usize) -> Result<Vec<NamedTxRequest>, ContenderError> {
        let mut templates: Vec<NamedTxRequest> = vec![];
        // find all {templates} and fetch their values from the DB
        let mut template_map = HashMap::<String, String>::new();

        // load values from [env] section
        if let Some(env) = &self.config.env {
            for (key, value) in env.iter() {
                template_map.insert(key.to_owned(), value.to_owned());
            }
        }
        let spam = self.config.spam.as_ref().ok_or(ContenderError::SpamError(
            "no spam configuration found",
            None,
        ))?;

        for function in spam.iter() {
            let func = alloy::json_abi::Function::parse(&function.signature).map_err(|e| {
                ContenderError::SpamError("failed to parse function name", Some(e.to_string()))
            })?;

            // hashmap to store fuzzy values
            let mut map = HashMap::<String, Vec<U256>>::new();

            // pre-generate fuzzy params
            if let Some(fuzz_args) = function.fuzz.as_ref() {
                // NOTE: This will only generate a single 32-byte value for each fuzzed parameter. Fuzzing values in arrays/structs is not yet supported.
                for fuzz_arg in fuzz_args.iter() {
                    let values = self
                        .seed
                        .seed_values(amount / spam.len(), fuzz_arg.min, fuzz_arg.max)
                        .map(|v| v.as_u256())
                        .collect::<Vec<U256>>();
                    map.insert(fuzz_arg.param.to_owned(), values);
                }
            }

            // find templates in fn args & `to`
            let fn_args = function.args.to_owned().unwrap_or_default();
            for arg in fn_args.iter() {
                find_template_values(&mut template_map, arg, self.db.as_ref())?;
            }
            find_template_values(&mut template_map, &function.to, self.db.as_ref())?;

            // generate spam txs; split total amount by number of spam steps
            for i in 0..(amount / spam.len()) {
                // encode function arguments
                let mut args = Vec::new();
                for j in 0..fn_args.len() {
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
                        let arg = &fn_args[j];
                        if arg.contains("{") {
                            replace_templates(arg, &template_map)
                        } else {
                            arg.to_owned()
                        }
                    });
                    args.push(val);
                } // args should have all template data filled now
                let input = encode_calldata(&args, &function.signature)?;

                // replace template value(s) for tx params
                let to = maybe_replace(&function.to, &template_map);
                let to = to.parse::<Address>().map_err(|e| {
                    ContenderError::SpamError("failed to parse address", Some(e.to_string()))
                })?;
                let from = function.from.parse::<Address>().map_err(|e| {
                    ContenderError::SpamError("failed to parse 'from' address", Some(e.to_string()))
                })?;
                let value = function
                    .value
                    .as_ref()
                    .map(|s| maybe_replace(s, &template_map))
                    .map(|s| s.parse::<U256>().ok())
                    .flatten();

                let tx = alloy::rpc::types::TransactionRequest {
                    to: Some(TxKind::Call(to)),
                    from: from.into(),
                    input: alloy::rpc::types::TransactionInput::both(input.into()),
                    value,
                    ..Default::default()
                };
                templates.push(tx.into());
            }
        }

        // interleave spam txs to evenly distribute various calls
        // this may create contention if different senders are specified for each call
        let spam_len = spam.len();
        let chunksize = templates.len() / spam_len;
        let mut new_templates = vec![];
        let max_idx = if templates.len() % spam_len != 0 {
            templates.len() - (templates.len() % spam_len)
        } else {
            templates.len() - 1
        };
        for i in 0..max_idx {
            let chunk_idx = chunksize * (i % spam_len);
            let idx = (i / spam_len) + chunk_idx;
            new_templates.push(templates[idx].to_owned());
        }

        Ok(new_templates)
    }
}

pub struct NilCallback;

impl NilCallback {
    pub fn new() -> Self {
        Self {}
    }
}

pub struct SetupCallback<D>
where
    D: DbOps,
{
    pub db: Arc<D>,
    pub rpc_provider: Arc<RpcProvider>,
}

pub struct LogCallback<D> {
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

impl<D> LogCallback<D>
where
    D: DbOps + Send + Sync + 'static,
{
    pub fn new(db: Arc<D>, rpc_provider: Arc<RpcProvider>) -> Self {
        Self { db, rpc_provider }
    }
}

impl OnTxSent for NilCallback {
    fn on_tx_sent(&self, _tx_res: TxHash, _name: Option<String>) -> Option<JoinHandle<()>> {
        // do nothing
        None
    }
}

impl<D> OnTxSent for LogCallback<D>
where
    D: DbOps + Send + Sync + 'static,
{
    fn on_tx_sent(&self, tx_hash: TxHash, run_id: Option<String>) -> Option<JoinHandle<()>> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("failed to get timestamp")
            .as_millis() as usize;
        let db = self.db.clone();
        let handle = spawn_task(async move {
            // TODO: get run ID to associate with `runs` table
            // let run_id = 1;

            db.insert_run_tx(
                run_id.map(|s| s.parse::<i64>().ok()).flatten().unwrap_or(0),
                tx_hash,
                timestamp,
            )
            .expect("failed to insert tx into db");
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
pub mod tests {
    use super::TestConfig;
    use super::*;
    use crate::db::sqlite::SqliteDb;
    use crate::generator::{
        types::{CreateDefinition, FunctionCallDefinition, FuzzParam},
        RandSeed,
    };
    use alloy::{hex::ToHexExt, primitives::Address};
    use std::{collections::HashMap, fs};

    pub fn get_testconfig() -> TestConfig {
        TestConfig {
            env: None,
            create: None,
            setup: None,
            spam: vec![FunctionCallDefinition {
                to: "0x7a250d5630B4cF539739dF2C5dAcb4c659F248DD".to_owned(),
                from: "0x7a250d5630B4cF539739dF2C5dAcb4c659F248DD".to_owned(),
                signature: "swap(uint256 x, uint256 y, address a, bytes b)".to_owned(),
                args: vec![
                    "1".to_owned(),
                    "2".to_owned(),
                    Address::repeat_byte(0x11).encode_hex(),
                    "0xdead".to_owned(),
                ]
                .into(),
                fuzz: None,
                value: None,
            }]
            .into(),
        }
    }

    pub fn get_fuzzy_testconfig() -> TestConfig {
        let fn_call = |data: &str, from_addr: &str| FunctionCallDefinition {
            to: "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D".to_owned(),
            from: from_addr.to_owned(),
            value: None,
            signature: "swap(uint256 x, uint256 y, address a, bytes b)".to_owned(),
            args: vec![
                "1".to_owned(),
                "2".to_owned(),
                Address::repeat_byte(0x11).encode_hex(),
                data.to_owned(),
            ]
            .into(),
            fuzz: vec![FuzzParam {
                param: "x".to_string(),
                min: None,
                max: None,
            }]
            .into(),
        };
        TestConfig {
            env: None,
            create: None,
            setup: None,
            spam: vec![
                fn_call("0xbeef", "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"),
                fn_call("0xea75", "0x70997970C51812dc3A010C7d01b50e0d17dc79C8"),
                fn_call("0xf00d", "0x3C44CdDdB6a900fa2b585dd299e03d12FA4293BC"),
            ]
            .into(),
        }
    }

    pub fn get_setup_testconfig() -> TestConfig {
        TestConfig {
            env: None,
            create: None,
            spam: None,
            setup: vec![
                FunctionCallDefinition {
                    to: "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D".to_owned(),
                    from: "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266".to_owned(),
                    value: Some("4096".to_owned()),
                    signature: "swap(uint256 x, uint256 y, address a, bytes b)".to_owned(),
                    args: vec![
                        "1".to_owned(),
                        "2".to_owned(),
                        Address::repeat_byte(0x11).encode_hex(),
                        "0xdead".to_owned(),
                    ]
                    .into(),
                    fuzz: None,
                },
                FunctionCallDefinition {
                    to: "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D".to_owned(),
                    from: "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266".to_owned(),
                    value: Some("0x1000".to_owned()),
                    signature: "swap(uint256 x, uint256 y, address a, bytes b)".to_owned(),
                    args: vec![
                        "1".to_owned(),
                        "2".to_owned(),
                        Address::repeat_byte(0x11).encode_hex(),
                        "0xbeef".to_owned(),
                    ]
                    .into(),
                    fuzz: None,
                },
            ]
            .into(),
        }
    }

    pub const COUNTER_BYTECODE: &'static str =
        "0x608060405234801561001057600080fd5b5060f78061001f6000396000f3fe6080604052348015600f57600080fd5b5060043610603c5760003560e01c80633fb5c1cb1460415780638381f58a146053578063d09de08a14606d575b600080fd5b6051604c3660046083565b600055565b005b605b60005481565b60405190815260200160405180910390f35b6051600080549080607c83609b565b9190505550565b600060208284031215609457600080fd5b5035919050565b60006001820160ba57634e487b7160e01b600052601160045260246000fd5b506001019056fea264697066735822122010f3077836fb83a22ad708a23102f2b487523767e1afef5a93c614619001648b64736f6c63430008170033";

    pub fn get_create_testconfig() -> TestConfig {
        let mut env = HashMap::new();
        env.insert("test1".to_owned(), "0xbeef".to_owned());
        env.insert("test2".to_owned(), "0x9001".to_owned());
        TestConfig {
            env: Some(env),
            create: Some(vec![CreateDefinition {
                bytecode: COUNTER_BYTECODE.to_string(),
                name: "test_counter".to_string(),
                from: "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266".to_owned(),
            }]),
            spam: None,
            setup: None,
        }
    }

    pub fn get_composite_testconfig() -> TestConfig {
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

    #[test]
    fn parses_testconfig_toml() {
        let test_file = TestConfig::from_file("univ2ConfigTest.toml").unwrap();
        assert!(test_file.env.is_some());
        assert!(test_file.setup.is_some());
        assert!(test_file.spam.is_some());
        let env = test_file.env.unwrap();
        let setup = test_file.setup.unwrap();
        let spam = test_file.spam.unwrap();

        assert_eq!(
            env.get("feeToSetter").unwrap(),
            "f39Fd6e51aad88F6F4ce6aB8827279cffFb92266"
        );
        assert_eq!(
            spam[0].from,
            "0x70997970C51812dc3A010C7d01b50e0d17dc79C8".to_owned()
        );
        assert_eq!(setup.len(), 11);
        assert_eq!(setup[0].value, Some("10000000000000000000".to_owned()));
        assert_eq!(spam[0].fuzz.as_ref().unwrap()[0].param, "amountIn");
        assert_eq!(
            spam[1].fuzz.as_ref().unwrap()[0].min,
            Some(U256::from(100000000))
        );
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
        let spam = cfg.clone().spam.unwrap();
        let args = spam[0].args.as_ref().unwrap();
        assert_eq!(spam[0].to, test_file2.spam.unwrap()[0].to);
        assert_eq!(args[0], "1");
        assert_eq!(args[1], "2");
        fs::remove_file("cargotest.toml").unwrap();
    }

    #[test]
    fn gets_spam_txs() {
        let test_file = get_testconfig();
        let seed = RandSeed::new();
        let test_gen = SpamGenerator::new(test_file, &seed, SqliteDb::new_memory());
        // this seed can be used to recreate the same test tx(s)
        let spam_txs = test_gen.get_txs(10).unwrap();
        // amount may be truncated if it doesn't divide evenly with the number of spam steps
        assert_eq!(spam_txs.len(), 9);
        let data = spam_txs[0].tx.input.input.to_owned().unwrap().to_string();
        assert_eq!(data, "0x022c0d9f00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000002000000000000000000000000111111111111111111111111111111111111111100000000000000000000000000000000000000000000000000000000000000800000000000000000000000000000000000000000000000000000000000000002dead000000000000000000000000000000000000000000000000000000000000");
    }

    #[test]
    fn fuzz_is_deterministic() {
        let test_file = get_fuzzy_testconfig();
        let seed = RandSeed::from_bytes(&[0x01; 32]);
        let test_gen = SpamGenerator::new(test_file, &seed, SqliteDb::new_memory());
        let num_txs = 12;
        let spam_txs_1 = test_gen.get_txs(num_txs).unwrap();
        let spam_txs_2 = test_gen.get_txs(num_txs).unwrap();
        for i in 0..spam_txs_1.len() {
            let data1 = spam_txs_1[i].tx.input.input.to_owned().unwrap().to_string();
            let data2 = spam_txs_2[i].tx.input.input.to_owned().unwrap().to_string();
            assert_eq!(data1, data2);
        }
    }
}
