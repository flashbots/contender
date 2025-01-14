use alloy::{
    network::{EthereumWallet, TransactionBuilder},
    primitives::{utils::format_ether, Address, U256},
    providers::{PendingTransactionConfig, Provider},
    rpc::types::TransactionRequest,
    signers::local::PrivateKeySigner,
};
use contender_core::{
    db::RunTx,
    generator::types::{AnyProvider, EthProvider, FunctionCallDefinition, SpamRequest},
    spammer::{LogCallback, NilCallback},
};
use contender_testfile::TestConfig;
use csv::Writer;
use std::{io::Write, str::FromStr, sync::Arc};
use termcolor::{ColorChoice, ColorSpec, StandardStream, WriteColor};

pub enum SpamCallbackType {
    Log(LogCallback),
    Nil(NilCallback),
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

pub fn get_create_pools(testconfig: &TestConfig) -> Vec<String> {
    testconfig
        .create
        .to_owned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|s| s.from_pool)
        .collect()
}

pub fn get_setup_pools(testconfig: &TestConfig) -> Vec<String> {
    testconfig
        .setup
        .to_owned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|s| s.from_pool)
        .collect()
}

pub fn get_spam_pools(testconfig: &TestConfig) -> Vec<String> {
    let mut from_pools = vec![];
    let spam = testconfig
        .spam
        .as_ref()
        .expect("No spam function calls found in testfile");

    for s in spam {
        match s {
            SpamRequest::Tx(fn_call) => {
                if let Some(from_pool) = &fn_call.from_pool {
                    from_pools.push(from_pool.to_owned());
                }
            }
            SpamRequest::Bundle(bundle) => {
                for tx in &bundle.txs {
                    if let Some(from_pool) = &tx.from_pool {
                        from_pools.push(from_pool.to_owned());
                    }
                }
            }
        }
    }

    // filter out non-unique pools
    from_pools.sort();
    from_pools.dedup();
    from_pools
}

pub fn get_signers_with_defaults(private_keys: Option<Vec<String>>) -> Vec<PrivateKeySigner> {
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
                fn_calls.push(fn_call.to_owned());
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
            let address = from.parse::<Address>().expect("invalid 'from' address");
            if prv_keys.iter().all(|k| k.address() != address) {
                panic!("No private key found for address: {}", address);
            }
        }
    }
}

async fn is_balance_sufficient(
    address: &Address,
    min_balance: U256,
    rpc_client: &AnyProvider,
) -> Result<bool, Box<dyn std::error::Error>> {
    let balance = rpc_client.get_balance(*address).await?;
    Ok(balance >= min_balance)
}

pub async fn fund_accounts(
    recipient_addresses: &[Address],
    fund_with: &PrivateKeySigner,
    rpc_client: &AnyProvider,
    eth_client: &EthProvider,
    min_balance: U256,
) -> Result<(), Box<dyn std::error::Error>> {
    let insufficient_balance_addrs =
        find_insufficient_balance_addrs(recipient_addresses, min_balance, rpc_client).await?;

    let mut pending_fund_txs = vec![];
    let admin_nonce = rpc_client
        .get_transaction_count(fund_with.address())
        .await?;
    for (idx, address) in insufficient_balance_addrs.iter().enumerate() {
        if !is_balance_sufficient(&fund_with.address(), min_balance, rpc_client).await? {
            // panic early if admin account runs out of funds
            return Err(format!(
                "Admin account {} has insufficient balance to fund this account.",
                fund_with.address()
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
                fund_with,
                *address,
                fund_amount,
                eth_client,
                Some(admin_nonce + idx as u64),
            )
            .await?,
        );
    }

    for tx in pending_fund_txs {
        let pending = rpc_client.watch_pending_transaction(tx).await?;
        println!("funding tx confirmed ({})", pending.await?);
    }

    Ok(())
}

pub async fn fund_account(
    sender: &PrivateKeySigner,
    recipient: Address,
    amount: U256,
    rpc_client: &EthProvider,
    nonce: Option<u64>,
) -> Result<PendingTransactionConfig, Box<dyn std::error::Error>> {
    println!(
        "funding account {} with user account {}",
        recipient,
        sender.address()
    );

    let gas_price = rpc_client.get_gas_price().await?;
    let nonce = nonce.unwrap_or(rpc_client.get_transaction_count(sender.address()).await?);
    let chain_id = rpc_client.get_chain_id().await?;
    let tx_req = TransactionRequest {
        from: Some(sender.address()),
        to: Some(alloy::primitives::TxKind::Call(recipient)),
        value: Some(amount),
        gas: Some(21000),
        gas_price: Some(gas_price + 4_200_000_000),
        nonce: Some(nonce),
        chain_id: Some(chain_id),
        ..Default::default()
    };
    let eth_wallet = EthereumWallet::from(sender.to_owned());
    let tx = tx_req.build(&eth_wallet).await?;
    let res = rpc_client.send_tx_envelope(tx).await?;

    Ok(res.into_inner())
}

/// Returns an error if any of the private keys do not have sufficient balance.
pub async fn find_insufficient_balance_addrs(
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

pub async fn spam_callback_default(
    log_txs: bool,
    rpc_client: Option<Arc<AnyProvider>>,
) -> SpamCallbackType {
    if let Some(rpc_client) = rpc_client {
        if log_txs {
            return SpamCallbackType::Log(LogCallback::new(rpc_client.clone()));
        }
    }
    SpamCallbackType::Nil(NilCallback)
}

pub fn write_run_txs<T: std::io::Write>(
    writer: &mut Writer<T>,
    txs: &[RunTx],
) -> Result<(), Box<dyn std::error::Error>> {
    for tx in txs {
        writer.serialize(tx)?;
    }
    writer.flush()?;
    Ok(())
}

pub fn prompt_cli(msg: impl AsRef<str>) -> String {
    let mut stdout = StandardStream::stdout(ColorChoice::Always);
    stdout
        .set_color(ColorSpec::new().set_fg(Some(termcolor::Color::Rgb(252, 186, 3))))
        .expect("failed to set stdout color");
    writeln!(&mut stdout, "{}", msg.as_ref()).expect("failed to write to stdout");
    stdout.reset().expect("failed to reset color");

    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .expect("Failed to read line");
    input.trim().to_owned()
}
