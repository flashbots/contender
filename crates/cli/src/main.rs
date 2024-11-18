mod commands;

use alloy::{
    network::{AnyNetwork, EthereumWallet, TransactionBuilder},
    primitives::{
        utils::{format_ether, parse_ether},
        Address, U256,
    },
    providers::{PendingTransactionConfig, Provider, ProviderBuilder},
    rpc::types::TransactionRequest,
    signers::local::PrivateKeySigner,
    transports::http::reqwest::Url,
};
use commands::{ContenderCli, ContenderSubcommand};
use contender_core::{
    agent_controller::{AgentStore, SignerStore},
    db::{DbOps, RunTx},
    generator::{
        types::{AnyProvider, EthProvider, FunctionCallDefinition, SpamRequest},
        RandSeed,
    },
    spammer::{BlockwiseSpammer, LogCallback, NilCallback, TimedSpammer},
    test_scenario::TestScenario,
};
use contender_sqlite::SqliteDb;
use contender_testfile::TestConfig;
use csv::{Writer, WriterBuilder};
use std::{
    str::FromStr,
    sync::{Arc, LazyLock},
};

static DB: LazyLock<SqliteDb> = std::sync::LazyLock::new(|| {
    SqliteDb::from_file("contender.db").expect("failed to open contender.db")
});

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = ContenderCli::parse_args();
    let _ = DB.create_tables(); // ignore error; tables already exist
    match args.command {
        ContenderSubcommand::Setup {
            testfile,
            rpc_url,
            private_keys,
            min_balance,
        } => {
            let url = Url::parse(rpc_url.as_ref()).expect("Invalid RPC URL");
            let rpc_client = ProviderBuilder::new()
                .network::<AnyNetwork>()
                .on_http(url.to_owned());
            let testconfig: TestConfig = TestConfig::from_file(&testfile)?;
            let min_balance = parse_ether(&min_balance)?;

            let user_signers = private_keys
                .as_ref()
                .unwrap_or(&vec![])
                .iter()
                .map(|key| PrivateKeySigner::from_str(&key).expect("invalid private key"))
                .collect::<Vec<PrivateKeySigner>>();
            let signers = get_signers_with_defaults(private_keys);
            check_private_keys(
                &testconfig.setup.to_owned().unwrap_or(vec![]),
                signers.as_slice(),
            );
            let broke_accounts = find_insufficient_balance_addrs(
                &user_signers.iter().map(|s| s.address()).collect::<Vec<_>>(),
                min_balance,
                &rpc_client,
            )
            .await?;
            if !broke_accounts.is_empty() {
                panic!("Some accounts do not have sufficient balance");
            }

            let scenario = TestScenario::new(
                testconfig.to_owned(),
                Arc::new(DB.clone()),
                url,
                None,
                RandSeed::new(),
                &signers,
                Default::default(),
            );

            scenario.deploy_contracts().await?;
            scenario.run_setup().await?;
            // TODO: catch failures and prompt user to retry specific steps
        }
        ContenderSubcommand::Spam {
            testfile,
            rpc_url,
            builder_url,
            txs_per_block,
            txs_per_second,
            duration,
            seed,
            private_keys,
            disable_reports,
            min_balance,
        } => {
            let testconfig = TestConfig::from_file(&testfile)?;
            let rand_seed = seed.map(|s| RandSeed::from_str(&s)).unwrap_or_default();
            let url = Url::parse(rpc_url.as_ref()).expect("Invalid RPC URL");
            let rpc_client = ProviderBuilder::new()
                .network::<AnyNetwork>()
                .on_http(url.to_owned());
            let eth_client = ProviderBuilder::new().on_http(url.to_owned());

            let duration = duration.unwrap_or_default();
            let min_balance = parse_ether(&min_balance)?;

            let user_signers = get_signers_with_defaults(private_keys);
            let spam = testconfig
                .spam
                .as_ref()
                .expect("No spam function calls found in testfile");

            // distill all FunctionCallDefinitions from the spam requests
            let mut fn_calls = vec![];
            // distill all from_pool arguments from the spam requests
            let mut from_pools = vec![];

            for s in spam {
                match s {
                    SpamRequest::Tx(fn_call) => {
                        fn_calls.push(fn_call.to_owned());
                        if let Some(from_pool) = &fn_call.from_pool {
                            from_pools.push(from_pool);
                        }
                    }
                    SpamRequest::Bundle(bundle) => {
                        fn_calls.extend(bundle.txs.iter().map(|s| {
                            if let Some(from_pool) = &s.from_pool {
                                from_pools.push(from_pool);
                            }
                            s.to_owned()
                        }));
                    }
                }
            }

            let mut agents = AgentStore::new();
            let signers_per_block = txs_per_block.unwrap_or(spam.len()) / spam.len();

            let mut all_signers = vec![];
            all_signers.extend_from_slice(&user_signers);

            for from_pool in from_pools {
                if agents.has_agent(from_pool) {
                    continue;
                }

                let agent = SignerStore::new_random(signers_per_block, &rand_seed);
                all_signers.extend_from_slice(&agent.signers);
                agents.add_agent(from_pool, agent);
            }

            check_private_keys(&fn_calls, &all_signers);

            let insufficient_balance_addrs = find_insufficient_balance_addrs(
                &all_signers.iter().map(|s| s.address()).collect::<Vec<_>>(),
                min_balance,
                &rpc_client,
            )
            .await?;

            let admin_signer = &user_signers[0];
            let mut pending_fund_txs = vec![];
            let admin_nonce = rpc_client
                .get_transaction_count(admin_signer.address())
                .await?;
            for (idx, address) in insufficient_balance_addrs.iter().enumerate() {
                if !is_balance_sufficient(&admin_signer.address(), min_balance, &rpc_client).await?
                {
                    // panic early if admin account runs out of funds
                    return Err(format!(
                        "Admin account {} has insufficient balance to fund this account.",
                        admin_signer.address()
                    )
                    .into());
                }

                let balance = rpc_client.get_balance(*address).await?;
                println!(
                    "Account {} has insufficient balance. (has {}, needed {})",
                    address,
                    format_ether(balance),
                    format_ether(min_balance)
                );

                let fund_amount = min_balance;
                pending_fund_txs.push(
                    fund_account(
                        &admin_signer,
                        *address,
                        fund_amount,
                        &eth_client,
                        Some(admin_nonce + idx as u64),
                    )
                    .await?,
                );
            }

            for tx in pending_fund_txs {
                let pending = rpc_client.watch_pending_transaction(tx).await?;
                println!("funding tx confirmed ({})", pending.await?);
            }

            if txs_per_block.is_some() && txs_per_second.is_some() {
                panic!("Cannot set both --txs-per-block and --txs-per-second");
            }
            if txs_per_block.is_none() && txs_per_second.is_none() {
                panic!("Must set either --txs-per-block (--tpb) or --txs-per-second (--tps)");
            }

            if let Some(txs_per_block) = txs_per_block {
                let scenario = TestScenario::new(
                    testconfig,
                    DB.clone().into(),
                    url,
                    builder_url.map(|url| Url::parse(&url).expect("Invalid builder URL")),
                    rand_seed,
                    &user_signers,
                    agents,
                );
                println!("Blockwise spamming with {} txs per block", txs_per_block);
                match spam_callback_default(!disable_reports, Arc::new(rpc_client).into()).await {
                    SpamCallbackType::Log(cback) => {
                        let timestamp = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .expect("Time went backwards")
                            .as_millis();
                        let run_id = DB.insert_run(timestamp as u64, txs_per_block * duration)?;
                        let mut spammer = BlockwiseSpammer::new(scenario, cback).await;
                        spammer
                            .spam_rpc(txs_per_block, duration, Some(run_id.into()))
                            .await?;
                        println!("Saved run. run_id = {}", run_id);
                    }
                    SpamCallbackType::Nil(cback) => {
                        let mut spammer = BlockwiseSpammer::new(scenario, cback).await;
                        spammer.spam_rpc(txs_per_block, duration, None).await?;
                    }
                };
                return Ok(());
            }

            // NOTE: private keys are not currently used for timed spamming.
            // Timed spamming only works with unlocked accounts, because it uses the `eth_sendTransaction` RPC method.
            let scenario = TestScenario::new(
                testconfig,
                DB.clone().into(),
                url,
                None,
                rand_seed,
                &user_signers,
                agents,
            );
            let tps = txs_per_second.unwrap_or(10);
            println!("Timed spamming with {} txs per second", tps);
            let spammer = TimedSpammer::new(scenario, NilCallback::new());
            spammer.spam_rpc(tps, duration).await?;
        }
        ContenderSubcommand::Report { id, out_file } => {
            let num_runs = DB.clone().num_runs()?;
            let id = if let Some(id) = id {
                if id == 0 || id > num_runs {
                    panic!("Invalid run ID: {}", id);
                }
                id
            } else {
                if num_runs == 0 {
                    panic!("No runs to report");
                }
                // get latest run
                println!("No run ID provided. Using latest run ID: {}", num_runs);
                num_runs
            };
            let txs = DB.clone().get_run_txs(id)?;
            println!("found {} txs", txs.len());
            println!(
                "Exporting report for run ID {:?} to out_file {:?}",
                id, out_file
            );

            if let Some(out_file) = out_file {
                let mut writer = WriterBuilder::new().has_headers(true).from_path(out_file)?;
                write_run_txs(&mut writer, &txs)?;
            } else {
                let mut writer = WriterBuilder::new()
                    .has_headers(true)
                    .from_writer(std::io::stdout());
                write_run_txs(&mut writer, &txs)?; // TODO: write a macro that lets us generalize the writer param to write_run_txs, then refactor this duplication
            };
        }
    }
    Ok(())
}

enum SpamCallbackType {
    Log(LogCallback),
    Nil(NilCallback),
}

/// Panics if any of the function calls' `from` addresses do not have a corresponding private key.
fn check_private_keys(fn_calls: &[FunctionCallDefinition], prv_keys: &[PrivateKeySigner]) {
    for fn_call in fn_calls {
        if let Some(from) = &fn_call.from {
            let address = from.parse::<Address>().expect("invalid 'from' address");
            if prv_keys.iter().all(|k| k.address() != address) {
                panic!("No private key found for address: {}", address);
            }
        }
    }
}

const DEFAULT_PRV_KEYS: [&str; 10] = [
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

fn get_signers_with_defaults(private_keys: Option<Vec<String>>) -> Vec<PrivateKeySigner> {
    if private_keys.is_none() {
        println!("No private keys provided. Using default private keys.");
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

async fn fund_account(
    admin_signer: &PrivateKeySigner,
    recipient: Address,
    amount: U256,
    rpc_client: &EthProvider,
    nonce: Option<u64>,
) -> Result<PendingTransactionConfig, Box<dyn std::error::Error>> {
    println!(
        "funding account {} with user account {}",
        recipient,
        admin_signer.address()
    );

    let gas_price = rpc_client.get_gas_price().await?;
    let nonce = nonce.unwrap_or(
        rpc_client
            .get_transaction_count(admin_signer.address())
            .await?,
    );
    let chain_id = rpc_client.get_chain_id().await?;
    let tx_req = TransactionRequest {
        from: Some(admin_signer.address()),
        to: Some(alloy::primitives::TxKind::Call(recipient)),
        value: Some(amount),
        gas: Some(21000),
        gas_price: Some(gas_price + 4_200_000_000),
        nonce: Some(nonce),
        chain_id: Some(chain_id),
        ..Default::default()
    };
    let eth_wallet = EthereumWallet::from(admin_signer.to_owned());
    let tx = tx_req.build(&eth_wallet).await?;
    let res = rpc_client.send_tx_envelope(tx).await?;

    Ok(res.into_inner())
}

async fn spam_callback_default(
    log_txs: bool,
    rpc_client: Option<Arc<AnyProvider>>,
) -> SpamCallbackType {
    if let Some(rpc_client) = rpc_client {
        if log_txs {
            return SpamCallbackType::Log(LogCallback::new(rpc_client.clone()));
        }
    }
    SpamCallbackType::Nil(NilCallback::new())
}

async fn is_balance_sufficient(
    address: &Address,
    min_balance: U256,
    rpc_client: &AnyProvider,
) -> Result<bool, Box<dyn std::error::Error>> {
    let balance = rpc_client.get_balance(*address).await?;
    Ok(balance >= min_balance)
}

/// Returns an error if any of the private keys do not have sufficient balance.
async fn find_insufficient_balance_addrs(
    addresses: &[Address],
    min_balance: U256,
    rpc_client: &AnyProvider,
) -> Result<Vec<Address>, Box<dyn std::error::Error>> {
    let mut insufficient_balance_addrs = vec![];
    for address in addresses {
        if !is_balance_sufficient(address, min_balance, rpc_client).await? {
            insufficient_balance_addrs.push(*address);
        }
    }
    Ok(insufficient_balance_addrs)
}

fn write_run_txs<T: std::io::Write>(
    writer: &mut Writer<T>,
    txs: &[RunTx],
) -> Result<(), Box<dyn std::error::Error>> {
    for tx in txs {
        writer.serialize(tx)?;
    }
    writer.flush()?;
    Ok(())
}
