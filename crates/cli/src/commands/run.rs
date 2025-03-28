use std::{env, str::FromStr};

use alloy::{
    consensus::TxType,
    eips::BlockId,
    network::AnyNetwork,
    providers::{DynProvider, Provider, ProviderBuilder},
    rpc::types::BlockTransactionsKind,
    transports::http::reqwest::Url,
};
use contender_core::{
    agent_controller::AgentStore,
    db::DbOps,
    error::ContenderError,
    generator::RandSeed,
    spammer::{LogCallback, Spammer, TimedSpammer},
    test_scenario::{TestScenario, TestScenarioParams},
};
use contender_testfile::TestConfig;

use crate::{
    default_scenarios::{BuiltinScenario, BuiltinScenarioConfig},
    util::{check_private_keys, get_signers_with_defaults, prompt_cli},
};

pub struct RunCommandArgs {
    pub scenario: BuiltinScenario,
    pub rpc_url: String,
    pub private_key: Option<String>,
    pub interval: usize,
    pub duration: usize,
    pub txs_per_duration: usize,
    pub skip_deploy_prompt: bool,
    pub tx_type: TxType,
}

pub async fn run(
    db: &(impl DbOps + Clone + Send + Sync + 'static),
    args: RunCommandArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    let user_signers = get_signers_with_defaults(args.private_key.map(|s| vec![s]));
    let admin_signer = &user_signers[0];
    let rand_seed = RandSeed::default();
    let provider = ProviderBuilder::new()
        .network::<AnyNetwork>()
        .on_http(Url::parse(&args.rpc_url).expect("Invalid RPC URL"));
    let block_gas_limit = provider
        .get_block(BlockId::latest(), BlockTransactionsKind::Hashes)
        .await?
        .map(|b| b.header.gas_limit)
        .ok_or(ContenderError::SetupError(
            "failed getting gas limit from block",
            None,
        ))?;

    let fill_percent = env::var("C_FILL_PERCENT")
        .map(|s| u16::from_str(&s).expect("invalid u16: fill_percent"))
        .unwrap_or(100u16);

    let scenario_config = match args.scenario {
        BuiltinScenario::FillBlock => BuiltinScenarioConfig::fill_block(
            block_gas_limit,
            args.txs_per_duration as u64,
            admin_signer.address(),
            fill_percent,
        ),
    };
    let scenario_name = scenario_config.to_string();
    let testconfig: TestConfig = scenario_config.into();
    check_private_keys(&testconfig, &user_signers);

    let rpc_url = Url::parse(&args.rpc_url).expect("Invalid RPC URL");
    let mut scenario = TestScenario::new(
        testconfig,
        db.clone().into(),
        rand_seed,
        TestScenarioParams {
            rpc_url: rpc_url.to_owned(),
            builder_rpc_url: None,
            signers: user_signers,
            agent_store: AgentStore::default(),
            tx_type: args.tx_type,
            gas_price_percent_add: None, // TODO: support this here !!!
        },
    )
    .await?;

    let contract_name = "SpamMe";
    let contract_result = db.get_named_tx(contract_name, rpc_url.as_str())?;

    let do_deploy_contracts = if contract_result.is_some() {
        if args.skip_deploy_prompt {
            false
        } else {
            let input = prompt_cli(format!(
                "{} deployment already detected. Re-deploy? [y/N]",
                contract_name
            ));
            input.to_lowercase() == "y"
        }
    } else {
        true
    };

    if do_deploy_contracts {
        println!("deploying contracts...");
        scenario.deploy_contracts().await?;
    }

    println!("running setup...");
    scenario.run_setup().await?;

    let wait_duration = std::time::Duration::from_secs(args.interval as u64);
    let spammer = TimedSpammer::new(wait_duration);
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("Time went backwards")
        .as_millis();
    let run_id = db.insert_run(
        timestamp as u64,
        args.duration * args.txs_per_duration,
        &format!("{} ({})", contract_name, scenario_name),
    )?;
    let callback = LogCallback::new(&DynProvider::new(provider.clone()));

    println!("starting spammer...");
    spammer
        .spam_rpc(
            &mut scenario,
            args.txs_per_duration,
            args.duration,
            Some(run_id),
            callback.into(),
        )
        .await?;

    Ok(())
}
