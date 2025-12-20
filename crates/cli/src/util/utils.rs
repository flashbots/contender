use crate::commands::error::{ArgsError, SetupError};
use crate::error::CliError;
use crate::util::error::ParseDurationError;
use crate::{commands::common::EngineParams, util::error::UtilError};
use alloy::{
    consensus::TxType,
    hex::{self, ToHexExt},
    network::{AnyTxEnvelope, EthereumWallet, TransactionBuilder},
    primitives::{Address, U256},
    providers::{PendingTransactionConfig, Provider},
    rpc::types::TransactionRequest,
    signers::local::PrivateKeySigner,
};
use contender_core::{
    generator::{
        types::{AnyProvider, FunctionCallDefinition, SpamRequest},
        util::complete_tx_request,
    },
    spammer::{LogCallback, NilCallback},
    util::get_blob_fee_maybe,
};
use contender_engine_provider::{ControlChain, DEFAULT_BLOCK_TIME};
use contender_testfile::TestConfig;
use nu_ansi_term::{AnsiGenericString, Color, Style as ANSIStyle};
use rand::Rng;
use std::path::PathBuf;
use std::{str::FromStr, sync::Arc, time::Duration};
use tracing::{debug, info, warn};

pub enum TypedSpamCallback {
    Log(LogCallback),
    Nil(NilCallback),
}

impl TypedSpamCallback {
    pub fn is_log(&self) -> bool {
        matches!(self, TypedSpamCallback::Log(_))
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

const DEFAULT_SCENARIOS_URL: &str =
    "https://raw.githubusercontent.com/flashbots/contender/refs/heads/main/scenarios";

/// Takes a testfile path or a builtin scenario and returns a TestConfig.
/// If the testfile starts with `scenario:`, it is treated as a builtin scenario.
/// Otherwise, it is treated as a file path.
/// Built-in scenarios are fetched relative to the default URL: [`DEFAULT_SCENARIOS_URL`](crate::util::DEFAULT_SCENARIOS_URL).
pub async fn load_testconfig(testfile: &str) -> Result<TestConfig, crate::CliError> {
    Ok(if testfile.starts_with("scenario:") {
        let remote_url = format!(
            "{DEFAULT_SCENARIOS_URL}/{}",
            testfile.replace("scenario:", "")
        );
        TestConfig::from_remote_url(&remote_url).await
    } else {
        let path: PathBuf = testfile.into();
        let parent = path
            .parent()
            .ok_or(UtilError::InvalidScenarioPath(testfile.to_owned()))?;
        Ok(TestConfig::from_file(testfile)?.with_scenario_directory(parent.into()))
    }?)
}

pub fn get_signers_with_defaults(private_keys: Option<Vec<String>>) -> Vec<PrivateKeySigner> {
    if private_keys.is_none() {
        warn!("No private keys provided. Using default private keys.");
    }
    let private_keys = private_keys.unwrap_or_default();
    let private_keys = [
        private_keys,
        DEFAULT_PRV_KEYS
            .into_iter()
            .map(|s| s.to_owned())
            .collect::<Vec<_>>(),
    ]
    .concat();

    private_keys
        .into_iter()
        .map(|k| PrivateKeySigner::from_str(&k).expect("Invalid private key"))
        .collect::<Vec<PrivateKeySigner>>()
}

pub fn check_private_keys(testconfig: &TestConfig, prv_keys: &[PrivateKeySigner]) {
    let setup = testconfig.setup.to_owned().unwrap_or_default();
    let spam = testconfig
        .spam
        .as_ref()
        .expect("No spam function calls found in testfile");

    // distill all FunctionCallDefinitions from the spam requests
    let mut fn_calls = vec![];

    for s in setup {
        fn_calls.push(s.to_owned());
    }

    for s in spam {
        match s {
            SpamRequest::Tx(fn_call) => {
                fn_calls.push(*fn_call.to_owned());
            }
            SpamRequest::Bundle(bundle) => {
                fn_calls.extend(bundle.txs.iter().map(|s| s.to_owned()));
            }
        }
    }

    check_private_keys_fns(&fn_calls, prv_keys);
}

/// Panics if any of the function calls' `from` addresses do not have a corresponding private key.
pub fn check_private_keys_fns(fn_calls: &[FunctionCallDefinition], prv_keys: &[PrivateKeySigner]) {
    for fn_call in fn_calls {
        if let Some(from) = &fn_call.from {
            // ignore placeholder values in the `from` field; you're on your own if you use those...
            if from.starts_with("{") {
                continue;
            }
            let address = from.parse::<Address>().expect("invalid 'from' address");
            if prv_keys.iter().all(|k| k.address() != address) {
                panic!("No private key found for address: {address}");
            }
        }
    }
}

async fn is_balance_sufficient(
    address: &Address,
    min_balance: U256,
    rpc_client: &AnyProvider,
) -> Result<(bool, U256), UtilError> {
    let balance = rpc_client.get_balance(*address).await?;
    Ok((balance >= min_balance, balance))
}

/// Funds given accounts if/when their balance is below the minimum balance.
///
/// TODO: remove this function
pub async fn fund_accounts(
    recipient_addresses: &[Address],
    fund_with: &PrivateKeySigner,
    rpc_client: &AnyProvider,
    min_balance: U256,
    tx_type: TxType,
    engine_params: &EngineParams,
) -> Result<(), CliError> {
    info!("Funding agent accounts from {}", fund_with.address());
    let EngineParams {
        engine_provider,
        call_fcu,
    } = engine_params;
    let insufficient_balances =
        find_insufficient_balances(recipient_addresses, min_balance, rpc_client).await?;

    let admin_nonce = rpc_client
        .get_transaction_count(fund_with.address())
        .await?;

    // pre-check if admin account has sufficient balance
    let gas_price = rpc_client.get_gas_price().await?;
    let gas_cost_per_tx = U256::from(21000) * U256::from(gas_price + (gas_price / 10));
    let chain_id = rpc_client.get_chain_id().await?;

    let total_cost = U256::from(insufficient_balances.len()) * (min_balance + gas_cost_per_tx);
    let (balance_sufficient, balance) =
        is_balance_sufficient(&fund_with.address(), total_cost, rpc_client).await?;
    if !balance_sufficient {
        return Err(UtilError::insufficient_user_funds(
            fund_with.address(),
            balance,
            total_cost,
            chain_id,
        )
        .into());
    }

    let mut fund_handles: Vec<tokio::task::JoinHandle<_>> = vec![];
    let (sender_pending_tx, mut receiver_pending_tx) =
        tokio::sync::mpsc::channel::<PendingTransactionConfig>(9000);

    let rpc_client = Arc::new(rpc_client.to_owned());

    let (balance_sufficient, balance) = is_balance_sufficient(
        &fund_with.address(),
        min_balance * U256::from(insufficient_balances.len()),
        &rpc_client,
    )
    .await?;
    if !balance_sufficient {
        // error early if admin account runs out of funds
        return Err(UtilError::insufficient_user_funds(
            fund_with.address(),
            balance,
            min_balance,
            chain_id,
        )
        .into());
    }

    if !insufficient_balances.is_empty() {
        let s = if insufficient_balances.len() == 1 {
            ""
        } else {
            "s"
        };
        info!(
            "sending funding txs ({} account{s})...",
            insufficient_balances.len()
        );
    }
    for (idx, (address, _)) in insufficient_balances.into_iter().enumerate() {
        let fund_amount = min_balance;
        let fund_with = fund_with.to_owned();
        let sender = sender_pending_tx.clone();
        let rpc_client = rpc_client.clone();

        fund_handles.push(tokio::task::spawn(async move {
            let res = fund_account(
                &fund_with,
                address,
                fund_amount,
                &rpc_client,
                Some(admin_nonce + idx as u64),
                tx_type,
            )
            .await?;
            sender.send(res).await.expect("failed to handle pending tx");

            Ok::<_, CliError>(())
        }));
    }

    if !fund_handles.is_empty() {
        info!("waiting for funding tasks to finish...");
        for handle in fund_handles {
            handle.await??;
        }
    }
    receiver_pending_tx.close();

    tokio::time::sleep(Duration::from_secs(DEFAULT_BLOCK_TIME)).await;

    let mut pending_txs = vec![];
    while let Some(tx) = receiver_pending_tx.recv().await {
        pending_txs.push(tx);
    }
    for txs_chunk in pending_txs.chunks(100) {
        if *call_fcu {
            if let Some(engine_provider) = &engine_provider {
                engine_provider.advance_chain(DEFAULT_BLOCK_TIME).await?;
            } else {
                return Err(ArgsError::EngineProviderUninitialized(
                    "required to advance chain".to_owned(),
                )
                .into());
            }
        }
        for tx in txs_chunk {
            // Use a timeout for funding transactions to prevent indefinite hanging
            // This prevents stalls when transactions get stuck in mempool or dropped
            let timeout_duration = Duration::from_secs(24);
            let tx_hash = *tx.tx_hash();

            let watch_result = tokio::time::timeout(timeout_duration, async {
                let pending = rpc_client.watch_pending_transaction(tx.to_owned()).await?;
                pending.await
            })
            .await;

            match watch_result {
                Ok(Ok(receipt)) => {
                    info!("funding tx confirmed ({})", receipt);
                }
                Ok(Err(e)) => {
                    return Err(SetupError::FundingTxFailed(tx_hash, e).into());
                }
                Err(_) => {
                    return Err(
                        SetupError::FundingTxTimedOut(tx_hash, timeout_duration.as_secs()).into(),
                    );
                }
            }
        }
    }

    Ok(())
}

pub async fn fund_account(
    sender: &PrivateKeySigner,
    recipient: Address,
    amount: U256,
    rpc_client: &AnyProvider,
    nonce: Option<u64>,
    tx_type: TxType,
) -> Result<PendingTransactionConfig, UtilError> {
    let gas_price = rpc_client.get_gas_price().await?;
    let blob_gas_price = get_blob_fee_maybe(rpc_client).await;
    let nonce = nonce.unwrap_or(rpc_client.get_transaction_count(sender.address()).await?);
    let chain_id = rpc_client.get_chain_id().await?;
    let mut tx_req = TransactionRequest {
        from: Some(sender.address()),
        to: Some(alloy::primitives::TxKind::Call(recipient)),
        value: Some(amount),
        nonce: Some(nonce),
        chain_id: Some(chain_id),
        ..Default::default()
    };
    complete_tx_request(
        &mut tx_req,
        tx_type,
        gas_price,
        gas_price / 10,
        21000,
        chain_id,
        blob_gas_price,
    );

    let eth_wallet = EthereumWallet::from(sender.to_owned());
    let tx = tx_req.build(&eth_wallet).await?;

    debug!(
        "funding account {recipient} with user account {}. tx: {}",
        sender.address(),
        tx.tx_hash().encode_hex()
    );
    let res = rpc_client
        .send_tx_envelope(AnyTxEnvelope::Ethereum(tx))
        .await?;

    Ok(res.into_inner())
}

/// Returns an error if any of the private keys do not have sufficient balance.
pub async fn find_insufficient_balances(
    addresses: &[Address],
    min_balance: U256,
    rpc_client: &AnyProvider,
) -> Result<Vec<(Address, U256)>, UtilError> {
    let mut insufficient_balances = vec![];
    for address in addresses {
        let (balance_sufficient, balance) =
            is_balance_sufficient(address, min_balance, rpc_client).await?;
        if !balance_sufficient {
            insufficient_balances.push((*address, balance));
        }
    }
    Ok(insufficient_balances)
}

pub fn spam_callback_default(
    log_txs: bool,
    send_fcu: bool,
    rpc_client: Option<Arc<AnyProvider>>,
    auth_client: Option<Arc<dyn ControlChain + Send + Sync + 'static>>,
    cancel_token: tokio_util::sync::CancellationToken,
) -> TypedSpamCallback {
    if let Some(rpc_client) = rpc_client {
        if log_txs {
            let log_callback = LogCallback {
                rpc_provider: rpc_client.clone(),
                auth_provider: auth_client,
                send_fcu,
                cancel_token,
            };
            return TypedSpamCallback::Log(log_callback);
        }
    }
    TypedSpamCallback::Nil(NilCallback)
}

pub fn prompt_cli(msg: impl AsRef<str>) -> String {
    println!("{}", Color::Rgb(252, 186, 3).paint(msg.as_ref()));

    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .expect("Failed to read line");
    input.trim().to_owned()
}

/// Prompts the user for a yes/no answer.
/// Returns true if the answer starts with 'y' or 'Y', false otherwise.
pub fn prompt_continue(msg: Option<&str>) -> bool {
    prompt_cli(msg.unwrap_or("Do you want to continue anyways? [y/N]"))
        .to_lowercase()
        .starts_with("y")
}

/// Returns the path to the data directory.
/// The directory is created if it does not exist.
pub fn data_dir() -> Result<String, UtilError> {
    let home_dir = if cfg!(windows) {
        std::env::var("USERPROFILE")?
    } else {
        std::env::var("HOME")?
    };

    let dir = format!("{home_dir}/.contender");

    // ensure directory exists
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Returns the fully-qualified path to the report directory.
pub fn init_reports_dir() -> String {
    let path = format!("{}/reports", data_dir().expect("invalid data directory"));
    std::fs::create_dir_all(&path).expect("failed to create report directory");
    path
}

/// Returns path to default contender DB file.
pub fn db_file() -> Result<String, UtilError> {
    let data_path = data_dir()?;
    Ok(format!("{data_path}/contender.db"))
}

pub fn bold<'a>(msg: impl AsRef<str> + 'a) -> AnsiGenericString<'a, str> {
    ANSIStyle::new().bold().paint(msg.as_ref().to_owned())
}

/// Parses a string with time units into a Duration.
/// Supported units: ms, msec, millisecond(s), s, sec(s), second(s), m, min(ute)(s), h, hr(s), hour(s), d, day(s).
pub fn parse_duration(input: &str) -> std::result::Result<Duration, ParseDurationError> {
    let s = input.trim().to_lowercase();

    // Split numeric part and unit part.
    let mut num_str = String::new();
    let mut unit_str = String::new();
    for c in s.chars() {
        if num_str.is_empty() && c.is_whitespace() {
            continue;
        }
        if c.is_ascii_digit() {
            if unit_str.is_empty() {
                num_str.push(c);
            } else {
                return Err(ParseDurationError::UnexpectedDigit(input.to_owned()));
            }
        } else if c == '.' {
            // For simplicity, we expect whole numbers.
            return Err(ParseDurationError::NoFloats(input.to_owned()));
        } else {
            unit_str.push(c);
        }
    }
    let value: u64 = num_str
        .parse()
        .map_err(|_| ParseDurationError::InvalidNumber(num_str))?;
    let unit = unit_str.trim();
    if unit.is_empty() {
        // No unit provided - default to seconds.
        return Ok(Duration::from_secs(value));
    }
    match unit {
        "ms" | "msec" | "msecs" | "millisecond" | "milliseconds" => {
            Ok(Duration::from_millis(value))
        }
        "s" | "sec" | "secs" | "second" | "seconds" => Ok(Duration::from_secs(value)),
        "m" | "min" | "mins" | "minute" | "minutes" => Ok(Duration::from_secs(value * 60)),
        "h" | "hr" | "hrs" | "hour" | "hours" => Ok(Duration::from_secs(value * 3600)),
        "d" | "day" | "days" => Ok(Duration::from_secs(value * 86400)),
        _ => Err(ParseDurationError::InvalidUnits(unit.to_owned())),
    }
}

pub fn load_seedfile() -> Result<String, CliError> {
    let data_path = data_dir()?;

    let seed_path = format!("{}/seed", &data_path);
    if !std::path::Path::new(&seed_path).exists() {
        info!("generating seed file at {}", &seed_path);
        let mut rng = rand::thread_rng();
        let seed: [u8; 32] = rng.gen();
        let seed_hex = hex::encode(seed);
        std::fs::write(&seed_path, seed_hex)?;
    }

    let stored_seed = format!("0x{}", std::fs::read_to_string(&seed_path)?);
    Ok(stored_seed)
}

/// Returns a human-readable "gas" string.
///
/// ## Example:
/// ```rs
/// assert_eq!(human_readable_gas(500), "500 gas");
/// assert_eq!(human_readable_gas(500_000_000), "500 Mgas");
/// assert_eq!(human_readable_gas(5_000_000_000), "5 Ggas");
/// ```
pub fn human_readable_gas(gas: u128) -> String {
    let unit_divisors = [
        ("Ggas", 1_000_000_000.0),
        ("Mgas", 1_000_000.0),
        ("Kgas", 1_000.0),
        ("gas", 1.0),
    ];
    let (gas_unit, divisor) = unit_divisors
        .iter()
        .find(|(_, divisor)| gas as f64 >= *divisor)
        .unwrap_or(unit_divisors.last().expect("empty unit_divisors"));
    format!("{} {gas_unit}", gas as f64 / divisor)
}

/// Returns a human-readable duration, which only goes up to minutes.
/// Doesn't display minutes until >2 minutes have elapsed.
///
/// ## Example:
/// ```rs
/// assert_eq!(human_readable_duration(Duration::from_secs(60)), "60 seconds");
/// assert_eq!(human_readable_duration(Duration::from_secs(240)), "4 minutes");
/// assert_eq!(human_readable_duration(Duration::from_millis(600)), "600 milliseconds");
/// ```
pub fn human_readable_duration(duration: Duration) -> String {
    if duration > Duration::from_secs(60 * 2) {
        format!("{} minutes", duration.as_secs_f32() / 60.0)
    } else if duration > Duration::from_secs(1) {
        format!("{} seconds", duration.as_secs_f32())
    } else {
        format!("{} milliseconds", duration.as_millis())
    }
}

#[cfg(test)]
mod test {
    use crate::error::CliError;
    use crate::util::error::UtilError;
    use crate::util::human_readable_duration;
    use crate::util::utils::human_readable_gas;

    use super::fund_accounts;
    use super::load_testconfig;
    use super::parse_duration;
    use alloy::{
        consensus::constants::ETH_TO_WEI,
        network::AnyNetwork,
        node_bindings::{Anvil, AnvilInstance},
        primitives::{Address, U256},
        providers::{DynProvider, Provider, ProviderBuilder},
        signers::local::PrivateKeySigner,
    };
    use std::str::FromStr;
    use std::time::Duration;

    pub fn spawn_anvil() -> AnvilInstance {
        Anvil::new().block_time_f64(0.25).spawn()
    }

    #[test]
    fn it_parses_durations() {
        let test_duration = |s: &str, d: Duration| {
            assert_eq!(parse_duration(s).unwrap(), d);
        };

        test_duration("1 second", Duration::from_secs(1));
    }

    #[test]
    fn test_parse_duration_no_unit_defaults_to_seconds() {
        // No unit provided should default to seconds.
        assert_eq!(parse_duration("2").unwrap(), Duration::from_secs(2));
        assert_eq!(parse_duration("   15   ").unwrap(), Duration::from_secs(15));
    }

    #[test]
    fn test_parse_duration_milliseconds() {
        let variants = [
            "5ms",
            "5 ms",
            "5msec",
            "5 msec",
            "5 millisecond",
            "5 milliseconds",
        ];
        for &variant in &variants {
            assert_eq!(
                parse_duration(variant).unwrap(),
                Duration::from_millis(5),
                "Failed on variant: {variant}"
            );
        }
    }

    #[test]
    fn test_parse_duration_seconds() {
        let variants = ["10s", "10 s", "10sec", "10 sec", "10 second", "10 seconds"];
        for &variant in &variants {
            assert_eq!(
                parse_duration(variant).unwrap(),
                Duration::from_secs(10),
                "Failed on variant: {variant}"
            );
        }
    }

    #[test]
    fn test_parse_duration_minutes() {
        let variants = ["3m", "3 m", "3min", "3 min", "3 minute", "3 minutes"];
        for &variant in &variants {
            assert_eq!(
                parse_duration(variant).unwrap(),
                Duration::from_secs(3 * 60),
                "Failed on variant: {variant}"
            );
        }
    }

    #[test]
    fn test_parse_duration_hours() {
        let variants = ["1h", "1 h", "1hr", "1 hr", "1 hour", "1 hours"];
        for &variant in &variants {
            assert_eq!(
                parse_duration(variant).unwrap(),
                Duration::from_secs(3600),
                "Failed on variant: {variant}"
            );
        }
    }

    #[test]
    fn test_parse_duration_days() {
        let variants = ["2d", "2 d", "2day", "2 day", "2days", "2 days"];
        for &variant in &variants {
            assert_eq!(
                parse_duration(variant).unwrap(),
                Duration::from_secs(2 * 86400),
                "Failed on variant: {variant}"
            );
        }
    }

    #[test]
    fn test_parse_duration_invalid_inputs() {
        // Floating point values are not supported.
        assert!(parse_duration("5.5s").is_err());
        // Missing numeric part.
        assert!(parse_duration("s").is_err());
        // Non-number input.
        assert!(parse_duration("abc").is_err());
        // Unrecognized unit.
        assert!(parse_duration("10 xs").is_err());
    }

    #[tokio::test]
    async fn fetch_bad_url() {
        let testconfig = load_testconfig("scenario:bad_path.toml").await;
        assert!(
            testconfig.is_err(),
            "Expected error when fetching non-existent URL"
        );
    }

    #[tokio::test]
    async fn fetch_correct_url_when_prefix_added() {
        let testconfig = load_testconfig("scenario:simpler.toml").await;
        assert!(testconfig.is_ok(), "Can't fetch this URL");
    }

    #[tokio::test]
    async fn dont_fetch_remote_scenario_without_prefix() {
        let testconfig = load_testconfig("bad_prefix:simpler.toml").await;
        assert!(testconfig.is_err(), "URL fetched even without prefix");
    }

    #[tokio::test]
    async fn fund_accounts_disallows_insufficient_balance() {
        let anvil = spawn_anvil();
        let rpc_client = DynProvider::new(
            ProviderBuilder::new()
                .network::<AnyNetwork>()
                .connect_http(anvil.endpoint_url()),
        );
        let min_balance = U256::from(ETH_TO_WEI);
        let default_signer = PrivateKeySigner::from_str(super::DEFAULT_PRV_KEYS[0]).unwrap();
        // address: 0x7E57f00F16dE6A0D6B720E9C0af5C869a1f71c66
        let new_signer = PrivateKeySigner::from_str(
            "0x08a418b870bf01990abc730a1cfc4ff04811f8e88bafa9edb8d40d802a33891f",
        )
        .unwrap();
        let recipient_addresses: Vec<Address> = [
            "0x0000000000000000000000000000000000000013",
            "0x7E57f00F16dE6A0D6B720E9C0af5C869a1f71c66",
        ]
        .iter()
        .map(|s| s.parse().unwrap())
        .collect();

        let tx_type = alloy::consensus::TxType::Eip1559;

        // send eth to the new signer
        fund_accounts(
            &recipient_addresses,
            &default_signer,
            &rpc_client,
            min_balance,
            tx_type,
            &Default::default(),
        )
        .await
        .unwrap();

        for addr in &recipient_addresses {
            let balance = rpc_client.get_balance(*addr).await.unwrap();
            println!("balance of {addr}: {balance}");
            assert_eq!(balance, U256::from(ETH_TO_WEI));
        }

        let res = fund_accounts(
            &["0x0000000000000000000000000000000000000014"
                .parse::<Address>()
                .unwrap()],
            &new_signer,
            &rpc_client,
            min_balance,
            tx_type,
            &Default::default(),
        )
        .await;
        println!("res: {res:?}");
        assert!(res.is_err());
        assert!(matches!(
            res.unwrap_err(),
            CliError::Util(UtilError::InsufficientUserFunds {
                sender: _,
                have: _,
                need: _,
                chain_id: _
            })
        ))
    }

    #[test]
    fn human_readable_gas_works() {
        assert_eq!(human_readable_gas(500), "500 gas");
        assert_eq!(human_readable_gas(500_000_000), "500 Mgas");
        assert_eq!(human_readable_gas(5_000_000_000), "5 Ggas");
        assert_eq!(human_readable_gas(5_101_000_000), "5.101 Ggas");
    }

    #[test]
    fn human_readable_duration_works() {
        assert_eq!(
            human_readable_duration(Duration::from_secs(60)),
            "60 seconds"
        );
        assert_eq!(
            human_readable_duration(Duration::from_secs(240)),
            "4 minutes"
        );
        assert_eq!(
            human_readable_duration(Duration::from_millis(600)),
            "600 milliseconds"
        );
    }
}
