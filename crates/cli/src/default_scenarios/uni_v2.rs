use std::str::FromStr;

use alloy::primitives::U256;
use clap::Parser;
use contender_core::{
    error::ContenderError,
    generator::{types::SpamRequest, CompiledContract, CreateDefinition, FunctionCallDefinition},
};
use contender_testfile::TestConfig;

use crate::{
    commands::common::parse_amount,
    default_scenarios::{builtin::ToTestConfig, contracts::test_token},
};

#[derive(Debug, Clone, Parser)]
pub struct UniV2CliArgs {
    #[arg(
        short,
        long,
        long_help = "The number of tokens to create in the scenario. Each token will be paired with WETH and each other token.",
        default_value = "2",
        value_name = "NUM_TOKENS",
        visible_aliases = &["tokens"]
    )]
    pub num_tokens: u32,

    #[arg(
        short,
        long,
        long_help = "The amount of ETH to deposit into each TOKEN pool. One additional multiple of this is also minted for trading.",
        default_value = "1 eth",
        value_name = "ETH_AMOUNT",
        value_parser = parse_amount,
        visible_aliases = &["weth"]
    )]
    pub weth_per_token: U256,

    #[arg(
        short,
        long,
        long_help = "The initial amount minted for each token. 50% of this will be deposited among trading pools. Units must be provided, e.g. '1 eth' to mint 1 token with 1e18 decimal precision.",
        default_value = "5000000 eth",
        value_parser = parse_amount,
        visible_aliases = &["mint"],
        value_name = "TOKEN_AMOUNT"
    )]
    pub initial_token_supply: U256,

    #[arg(
        long,
        long_help = "The amount of WETH to trade in the scenario. If not provided, 0.01% of the pool's initial WETH will be traded for each token. Units must be provided, e.g. '0.1 eth'.",
        value_parser = parse_amount,
        value_name = "WETH_AMOUNT",
        visible_aliases = &["trade-weth"]
    )]
    pub weth_trade_amount: Option<U256>,

    #[arg(
        long,
        long_help = "The amount of tokens to trade in the scenario. If not provided, 0.01% of the initial supply will be traded for each token.",
        value_parser = parse_amount,
        value_name = "TOKEN_AMOUNT",
        visible_aliases = &["trade-token"]
    )]
    pub token_trade_amount: Option<U256>,
}

#[derive(Debug, Clone)]
pub struct UniV2Args {
    /// The number of tokens to create in the scenario. Each token will be paired with WETH and each other token.
    pub num_tokens: u32,
    /// The amount of ETH to deposit into each TOKEN/WETH pool.
    pub weth_per_token: U256,
    /// The initial amount minted for each token.
    pub initial_token_supply: U256,
    /// The amount of WETH to trade in the scenario. If not provided, 0.01% of the pool's initial WETH will be traded for each token.
    pub weth_trade_amount: U256,
    /// The amount of tokens to trade in the scenario. If not provided, 0.01% of the initial supply will be traded for each token.
    pub token_trade_amount: U256,
}

impl UniV2Args {
    pub fn liquidity_amount(&self) -> U256 {
        self.initial_token_supply / U256::from(2)
    }
    pub fn pool_liquidity_amount(&self) -> U256 {
        self.liquidity_amount() / U256::from(self.num_tokens + 1)
    }
}

impl From<UniV2CliArgs> for UniV2Args {
    fn from(args: UniV2CliArgs) -> Self {
        UniV2Args {
            num_tokens: args.num_tokens,
            weth_per_token: args.weth_per_token,
            initial_token_supply: args.initial_token_supply,
            weth_trade_amount: args
                .weth_trade_amount
                .unwrap_or(args.weth_per_token / U256::from(10_000)), // default to 0.01% of the pool's initial WETH
            token_trade_amount: args
                .token_trade_amount
                .unwrap_or(args.initial_token_supply / U256::from(10_000)), // default to 0.01% of the initial supply
        }
    }
}

impl ToTestConfig for UniV2Args {
    fn to_testconfig(&self) -> TestConfig {
        let mut config = TestConfig::from_str(include_str!("../../../../scenarios/uniV2.toml"))
            .expect("invalid file");

        // this will be updated as we modify the create steps
        let mut create_steps = config.create.unwrap_or_default();

        // remove token deployment steps from the create steps, we'll re-make them dynamically
        create_steps.retain(|c| !c.contract.name.starts_with("testToken"));

        // add new token deployment steps to the create steps at the beginning of the list
        let mut add_create_steps = vec![];
        for i in 0..self.num_tokens {
            let deployment = CreateDefinition {
                contract: test_token(i + 1, self.initial_token_supply),
                from: None,
                from_pool: Some("admin".to_owned()),
            };
            add_create_steps.push(deployment);
        }
        create_steps.splice(0..0, add_create_steps);

        // now that contract updates are done, we can update the config & use contracts in setup steps
        config.create = Some(create_steps.to_owned());
        let find_contract = |name: &str| {
            let contract = create_steps
                .iter()
                .find(|c| c.contract.name == name)
                .ok_or(ContenderError::SetupError(
                    "contract not found in create steps:",
                    Some(name.to_owned()),
                ))?
                .contract
                .to_owned();
            Ok::<_, ContenderError>(contract)
        };

        let weth = find_contract("weth").expect("contract");
        let univ2_factory = find_contract("uniV2Factory").expect("contract");
        let uni_router_v2 = find_contract("uniRouterV2").expect("contract");
        let unicheat = find_contract("unicheat").expect("contract");

        // setup & spam steps will be replaced so that we can work with a dynamic number of tokens
        let weth_value = self.weth_per_token * U256::from(self.num_tokens + 1);
        let mut setup_steps = vec![FunctionCallDefinition::new(weth.template_name())
            .with_signature("deposit()")
            .with_from_pool("admin")
            .with_kind("weth_deposit")
            .with_value(weth_value)];
        // we'll make a spam step for every token pair and trade direction
        let mut spam_steps = vec![];

        /* FunctionCallDefinition helpers */
        let create_pair = |token_a: &CompiledContract, token_b: &CompiledContract| {
            FunctionCallDefinition::new(univ2_factory.template_name())
                .with_signature("createPair(address,address)")
                .with_from_pool("admin")
                .with_kind(format!(
                    "univ2_create_pair_{}-{}",
                    token_a.name, token_b.name
                ))
                .with_args(&[token_a.template_name(), token_b.template_name()])
        };
        let add_liquidity = |token_a: &CompiledContract,
                             token_b: &CompiledContract,
                             amount_a: U256,
                             amount_b: U256| {
            let deadline = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("Time went backwards")
                .as_secs()
                + 600;

            FunctionCallDefinition::new(uni_router_v2.template_name())
                .with_signature(
                    "addLiquidity(address,address,uint256,uint256,uint256,uint256,address,uint256)",
                )
                .with_from_pool("admin")
                .with_kind(format!(
                    "univ2_add_liquidity_{}-{}",
                    token_a.name, token_b.name
                ))
                .with_args(&[
                    token_a.template_name(),
                    token_b.template_name(),
                    amount_a.to_string(),
                    amount_b.to_string(),
                    (amount_a / U256::from(100)).to_string(), // 1% slippage
                    (amount_b / U256::from(100)).to_string(), // 1% slippage
                    "{_sender}".to_owned(),
                    deadline.to_string(),
                ])
        };
        let transfer = |token: &CompiledContract, to: &CompiledContract, amount: U256| {
            FunctionCallDefinition::new(token.template_name())
                .with_signature("transfer(address,uint256)")
                .with_from_pool("admin")
                .with_kind(format!("{}_transfer_to_{}", token.name, to.name))
                .with_args(&[to.template_name(), amount.to_string()])
        };
        let approve_max = |token: &CompiledContract, spender: &CompiledContract| {
            FunctionCallDefinition::new(token.template_name())
                .with_signature("approve(address,uint256)")
                .with_from_pool("admin")
                .with_kind(format!("{}_approve_{}", token.name, spender.name))
                .with_args(&[spender.template_name(), U256::MAX.to_string()])
        };
        let swap = |token_a: &CompiledContract, token_b: &CompiledContract, amount: U256| {
            FunctionCallDefinition::new(unicheat.template_name())
                .with_signature("swap(address,address,uint256)")
                .with_from_pool("spammers")
                .with_kind(format!("univ2_swap_{}-{}", token_a.name, token_b.name))
                .with_args(&[
                    token_a.template_name(),
                    token_b.template_name(),
                    amount.to_string(),
                ])
        };

        // approve the router to spend WETH
        setup_steps.push(approve_max(&weth, &uni_router_v2));

        // fund unicheat with WETH
        let amt = self.weth_per_token;
        setup_steps.push(transfer(&weth, &unicheat, amt));

        // approve router to spend tokens
        for token in 0..self.num_tokens {
            let token_contract = find_contract(&format!("testToken{}", token + 1)).expect("token");
            setup_steps.push(approve_max(&token_contract, &uni_router_v2));
        }
        setup_steps.push(approve_max(&weth, &uni_router_v2));

        // setup each token/weth pair, and omni-directional token/token pairs
        for i in 0..self.num_tokens {
            let token_name = format!("testToken{}", i + 1);
            let token = find_contract(&token_name).expect("token");

            // create WETH/TOKEN pair
            setup_steps.push(create_pair(&weth, &token));

            // add liquidity for the token/WETH pair
            setup_steps.push(add_liquidity(
                &weth,
                &token,
                self.weth_per_token,
                self.pool_liquidity_amount(),
            ));

            // swap WETH for the token
            spam_steps.push(swap(&weth, &token, self.weth_trade_amount));

            // swap the token for WETH
            spam_steps.push(swap(&token, &weth, self.weth_trade_amount));

            // create pair with other tokens
            for j in (i + 1)..self.num_tokens {
                let other_token_name = format!("testToken{}", j + 1);
                let other_token = find_contract(&other_token_name).expect("token not found");

                // create token/token pair
                setup_steps.push(create_pair(&token, &other_token));

                // add liquidity for token/token pair
                setup_steps.push(add_liquidity(
                    &token,
                    &other_token,
                    self.pool_liquidity_amount(),
                    self.pool_liquidity_amount(),
                ));

                // swap the token for the other token
                spam_steps.push(swap(&token, &other_token, self.token_trade_amount));

                // swap the other token for the token
                spam_steps.push(swap(&other_token, &token, self.token_trade_amount));
            }

            // fund unicheat with remaining tokens
            setup_steps.push(transfer(&token, &unicheat, self.liquidity_amount()));
        }

        // replace steps in the config
        config.setup = Some(setup_steps);
        config.spam = Some(
            spam_steps
                .into_iter()
                .map(Box::new)
                .map(SpamRequest::Tx)
                .collect::<Vec<_>>(),
        );

        config
    }
}
