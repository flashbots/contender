use crate::{
    agent_controller::{AgentStore, SignerRegistry},
    db::DbOps,
    error::ContenderError,
    generator::{
        seeder::{SeedValue, Seeder},
        templater::Templater,
        types::{CreateDefinition, FunctionCallDefinition, FuzzParam},
    },
    Result,
};
use alloy::{
    hex::ToHexExt,
    primitives::{Address, U256},
};
use async_trait::async_trait;
use named_txs::ExecutionRequest;
pub use named_txs::NamedTxRequestBuilder;
pub use seeder::rand_seed::RandSeed;
use std::{collections::HashMap, fmt::Debug, hash::Hash};
use types::{CreateDefinitionStrict, FunctionCallDefinitionStrict, SpamRequest};

pub use types::{CallbackResult, NamedTxRequest, PlanType};

/// Defines named tx requests, which are used to store transaction requests with optional names and kinds.
/// Used for tracking transactions in a test scenario.
pub mod named_txs;

/// Generates values for fuzzed parameters.
/// Contains the Seeder trait and an implementation.
pub mod seeder;

/// Provides templating for transaction requests, etc.
/// Contains the Templater trait and an implementation.
pub mod templater;

/// Contains types used by the generator module.
pub mod types;

/// Utility functions used in the generator module.
pub mod util;

const VALUE_KEY: &str = "__tx_value_contender__";

pub trait PlanConfig<K>
where
    K: Eq + Hash + Debug + Send + Sync,
{
    /// Get \[\[env]] variables from the plan configuration.
    fn get_env(&self) -> Result<HashMap<K, String>>;

    /// Get contract-creation steps from the plan configuration.
    fn get_create_steps(&self) -> Result<Vec<CreateDefinition>>;

    /// Get setup transactions from the plan configuration.
    fn get_setup_steps(&self) -> Result<Vec<FunctionCallDefinition>>;

    /// Get spam step templates from the plan configuration.
    fn get_spam_steps(&self) -> Result<Vec<SpamRequest>>;
}

fn parse_map_key(fuzz: FuzzParam) -> Result<String> {
    if fuzz.param.is_none() && fuzz.value.is_none() {
        return Err(ContenderError::SpamError(
            "fuzz must specify either `param` or `value`",
            None,
        ));
    }
    if fuzz.param.is_some() && fuzz.value.is_some() {
        return Err(ContenderError::SpamError(
            "fuzz cannot specify both `param` and `value`; choose one per fuzz directive",
            None,
        ));
    }

    let key = if let Some(param) = &fuzz.param {
        param.to_owned()
    } else if let Some(value) = fuzz.value {
        if !value {
            return Err(ContenderError::SpamError(
                "fuzz.value is false, but no param is specified",
                None,
            ));
        }
        VALUE_KEY.to_owned()
    } else {
        return Err(ContenderError::SpamError("this should never happen", None));
    };

    Ok(key)
}

#[async_trait]
pub trait Generator<K, D, T>
where
    K: Eq + Hash + Debug + ToString + ToOwned<Owned = K> + Send + Sync,
    D: Send + Sync + DbOps,
    T: Send + Sync + Templater<K>,
{
    fn get_plan_conf(&self) -> &impl PlanConfig<K>;
    fn get_templater(&self) -> &T;
    fn get_db(&self) -> &D;
    fn get_fuzz_seeder(&self) -> &impl Seeder;
    fn get_agent_store(&self) -> &AgentStore;
    fn get_rpc_url(&self) -> String;

    /// Generates a map of N=`num_values` fuzzed values for each parameter in `fuzz_args`.
    fn create_fuzz_map(
        &self,
        num_values: usize,
        fuzz_args: &[FuzzParam],
    ) -> Result<HashMap<String, Vec<U256>>> {
        let seed = self.get_fuzz_seeder();
        let mut map = HashMap::<String, Vec<U256>>::new();

        for fuzz in fuzz_args.iter() {
            let key = parse_map_key(fuzz.to_owned())?;
            map.insert(
                key,
                seed.seed_values(num_values, fuzz.min, fuzz.max)
                    .map(|v| v.as_u256())
                    .collect(),
            );
        }

        Ok(map)
    }

    fn make_strict_create(
        &self,
        create_def: &CreateDefinition,
        idx: usize,
    ) -> Result<CreateDefinitionStrict> {
        let agents = self.get_agent_store();
        let from_address: Address = if let Some(from_pool) = &create_def.from_pool {
            let agent = agents
                .get_agent(from_pool)
                .ok_or(ContenderError::SpamError(
                    "from_pool not found in agent store",
                    Some(from_pool.to_owned()),
                ))?;
            agent.get_address(idx).ok_or(ContenderError::SpamError(
                "signer not found in agent store",
                Some(format!("from_pool={}, idx={}", from_pool, idx)),
            ))?
        } else if let Some(from) = &create_def.from {
            from.parse().map_err(|e| {
                ContenderError::SpamError(
                    "failed to parse 'from' address",
                    Some(format!("from={}, error={}", from, e)),
                )
            })?
        } else {
            return Err(ContenderError::SpamError(
                "invalid runtime config: must specify 'from' or 'from_pool'",
                None,
            ));
        };

        let bytecode = create_def
            .bytecode
            .to_owned()
            .replace("{_sender}", &from_address.encode_hex()); // inject address WITHOUT 0x prefix

        Ok(CreateDefinitionStrict {
            name: create_def.name.to_owned(),
            bytecode,
            from: from_address,
        })
    }

    fn make_strict_call(
        &self,
        funcdef: &FunctionCallDefinition,
        idx: usize,
    ) -> Result<FunctionCallDefinitionStrict> {
        let agents = self.get_agent_store();
        let from_address: Address = if let Some(from_pool) = &funcdef.from_pool {
            let agent = agents
                .get_agent(from_pool)
                .ok_or(ContenderError::SpamError(
                    "from_pool not found in agent store",
                    Some(from_pool.to_owned()),
                ))?;
            agent.get_address(idx).ok_or(ContenderError::SpamError(
                "signer not found in agent store",
                Some(format!("from_pool={}, idx={}", from_pool, idx)),
            ))?
        } else if let Some(from) = &funcdef.from {
            from.parse().map_err(|e| {
                ContenderError::SpamError(
                    "failed to parse 'from' address",
                    Some(format!("from={}, error={}", from, e)),
                )
            })?
        } else {
            return Err(ContenderError::SpamError(
                "invalid runtime config: must specify 'from' or 'from_pool'",
                None,
            ));
        };

        // manually replace {_sender} with the 'from' address
        let args = funcdef.args.to_owned().unwrap_or_default();
        let args = args
            .iter()
            .map(|arg| {
                if arg.contains("{_sender}") {
                    // return `from` address WITH 0x prefix
                    arg.replace("{_sender}", &from_address.to_string())
                } else {
                    arg.to_owned()
                }
            })
            .collect::<Vec<String>>();

        let to_address = if funcdef.to == "{_sender}" {
            from_address.to_string()
        } else {
            funcdef.to.to_owned()
        };

        Ok(FunctionCallDefinitionStrict {
            to: to_address,
            from: from_address,
            signature: funcdef.signature.to_owned(),
            args,
            value: funcdef.value.to_owned(),
            fuzz: funcdef.fuzz.to_owned().unwrap_or_default(),
            kind: funcdef.kind.to_owned(),
        })
    }

    async fn load_txs<F: Send + Sync + Fn(NamedTxRequest) -> CallbackResult>(
        &self,
        plan_type: PlanType<F>,
    ) -> Result<Vec<ExecutionRequest>> {
        let conf = self.get_plan_conf();
        let env = conf.get_env().unwrap_or_default();
        let db = self.get_db();
        let templater = self.get_templater();

        let mut placeholder_map = HashMap::<K, String>::new();
        for (key, value) in env.iter() {
            placeholder_map.insert(key.to_owned(), value.to_owned());
        }

        let mut txs: Vec<ExecutionRequest> = vec![];

        match plan_type {
            PlanType::Create(on_create_step) => {
                let create_steps = conf.get_create_steps()?;

                // txs will be grouped by account [from=1, from=1, from=1, from=2, from=2, from=2, ...]
                for step in create_steps.iter() {
                    // populate step with from address
                    let step = self.make_strict_create(step, 0)?;

                    // lookup placeholder values in DB & update map before templating
                    templater.find_placeholder_values(
                        &step.bytecode,
                        &mut placeholder_map,
                        db,
                        &self.get_rpc_url(),
                    )?;

                    // create tx with template values
                    let tx = NamedTxRequestBuilder::new(
                        templater.template_contract_deploy(&step, &placeholder_map)?,
                    )
                    .with_name(&step.name)
                    .build();

                    let handle = on_create_step(tx.to_owned())?;
                    if let Some(handle) = handle {
                        handle.await.map_err(|e| {
                            ContenderError::with_err(e, "join error; callback crashed")
                        })?;
                    }
                    txs.push(tx.into());
                }
            }
            PlanType::Setup(on_setup_step) => {
                let setup_steps = conf.get_setup_steps()?;

                // txs will be grouped by account [from=1, from=1, from=1, from=2, from=2, from=2, ...]
                let rpc_url = self.get_rpc_url();

                for step in setup_steps.iter() {
                    // lookup placeholders in DB & update map before templating
                    templater.find_fncall_placeholders(step, db, &mut placeholder_map, &rpc_url)?;

                    // setup tx with template values
                    let tx = NamedTxRequest::new(
                        templater.template_function_call(
                            &self.make_strict_call(step, 0)?, // 'from' address injected here
                            &placeholder_map,
                        )?,
                        None,
                        step.kind.to_owned(),
                    );

                    let handle = on_setup_step(tx.to_owned())?;
                    if let Some(handle) = handle {
                        handle.await.map_err(|e| {
                            ContenderError::with_err(e, "join error; callback crashed")
                        })?;
                    }
                    txs.push(tx.into());
                }
            }
            PlanType::Spam(num_txs, on_spam_setup) => {
                let spam_steps = conf.get_spam_steps()?;
                let num_steps = spam_steps.len();
                // round num_txs up to the nearest multiple of num_steps to prevent missed steps
                let num_txs = num_txs + (num_txs % num_steps);
                let mut placeholder_map = HashMap::<K, String>::new();
                let mut canonical_fuzz_map = HashMap::<String, Vec<U256>>::new();

                // finds fuzzed values for a function call definition and populates `canonical_fuzz_map` with fuzzy values.
                let mut find_fuzz = |req: &FunctionCallDefinition| {
                    let fuzz_args = req.fuzz.to_owned().unwrap_or_default();
                    let fuzz_map = self.create_fuzz_map(num_txs, &fuzz_args)?; // this may create more values than needed, but it's fine
                    canonical_fuzz_map.extend(fuzz_map);
                    Ok(())
                };

                // finds placeholders in a function call definition and populates `placeholder_map` and `canonical_fuzz_map` with injectable values.
                let rpc_url = self.get_rpc_url();
                let mut lookup_tx_placeholders = |tx: &FunctionCallDefinition| {
                    let res =
                        templater.find_fncall_placeholders(tx, db, &mut placeholder_map, &rpc_url);
                    if let Err(e) = res {
                        eprintln!("error finding placeholders: {}", e);
                        return Err(ContenderError::SpamError(
                            "failed to find placeholder value",
                            Some(e.to_string()),
                        ));
                    }
                    find_fuzz(tx)?;
                    Ok(())
                };

                for step in spam_steps.iter() {
                    // populate placeholder map for each step
                    match step {
                        SpamRequest::Tx(tx) => {
                            lookup_tx_placeholders(tx)?;
                        }
                        SpamRequest::Bundle(req) => {
                            for tx in req.txs.iter() {
                                lookup_tx_placeholders(tx)?;
                            }
                        }
                    };
                }

                let agentstore = self.get_agent_store();
                let num_accts = agentstore
                    .all_agents()
                    .next()
                    .map(|(_, store)| store.signers.len())
                    .unwrap_or(1);
                if num_accts == 0 {
                    return Err(ContenderError::SpamError(
                        "no accounts found in agent store",
                        None,
                    ));
                }

                // txs will be grouped by step [from=1, from=2, from=3, from=1, from=2, from=3, ...]
                for step in spam_steps.iter() {
                    for i in 0..(num_txs / num_steps) {
                        // converts a FunctionCallDefinition to a NamedTxRequest (filling in fuzzable args),
                        // returns a callback handle and the processed tx request
                        let prepare_tx = |req| {
                            let args = get_fuzzed_args(req, &canonical_fuzz_map, i);
                            let fuzz_tx_value = get_fuzzed_tx_value(req, &canonical_fuzz_map, i);
                            let mut req = req.to_owned();
                            req.args = Some(args);

                            if fuzz_tx_value.is_some() {
                                req.value = fuzz_tx_value;
                            }

                            let tx = NamedTxRequest::new(
                                templater.template_function_call(
                                    &self.make_strict_call(&req, i % num_accts)?, // 'from' address injected here
                                    &placeholder_map,
                                )?,
                                None,
                                req.kind.to_owned(),
                            );
                            Ok((on_spam_setup(tx.to_owned())?, tx))
                        };

                        match step {
                            SpamRequest::Tx(req) => {
                                let (handle, tx) = prepare_tx(req)?;
                                if let Some(handle) = handle {
                                    handle.await.map_err(|e| {
                                        ContenderError::with_err(e, "error from callback")
                                    })?;
                                }
                                txs.push(tx.into());
                            }
                            SpamRequest::Bundle(req) => {
                                let mut bundle_txs = vec![];
                                for tx in req.txs.iter() {
                                    let (handle, txr) = prepare_tx(tx)?;
                                    if let Some(handle) = handle {
                                        handle.await.map_err(|e| {
                                            ContenderError::with_err(e, "error from callback")
                                        })?;
                                    }
                                    bundle_txs.push(txr);
                                }
                                txs.push(bundle_txs.into());
                            }
                        }
                    }
                }
            }
        }

        Ok(txs)
    }
}

/// For the given function call definition, return the fuzzy arguments for the given fuzz index.
fn get_fuzzed_args(
    tx: &FunctionCallDefinition,
    fuzz_map: &HashMap<String, Vec<U256>>,
    fuzz_idx: usize,
) -> Vec<String> {
    let func = alloy::json_abi::Function::parse(&tx.signature)
        .expect("[get_fuzzed_args] failed to parse function signature");
    let tx_args = tx.args.as_deref().unwrap_or_default();
    tx_args
        .iter()
        .enumerate()
        .map(|(idx, arg)| {
            let maybe_fuzz = || {
                let input_def = func.inputs[idx].to_string();
                // there's probably a better way to do this, but I haven't found it
                // we're looking for something like "uint256 arg_name" in input_def
                let arg_namedefs = input_def.split_ascii_whitespace().collect::<Vec<&str>>();
                if arg_namedefs.len() < 2 {
                    // can't fuzz unnamed params
                    return None;
                }
                let arg_name = arg_namedefs[1];
                if fuzz_map.contains_key(arg_name) {
                    return Some(
                        fuzz_map.get(arg_name).expect("this should never happen")[fuzz_idx]
                            .to_string(),
                    );
                }
                None
            };

            // !!! args with template values will be overwritten by the fuzzer if it's enabled for this arg
            maybe_fuzz().unwrap_or(arg.to_owned())
        })
        .collect()
}

fn get_fuzzed_tx_value(
    tx: &FunctionCallDefinition,
    fuzz_map: &HashMap<String, Vec<U256>>,
    fuzz_idx: usize,
) -> Option<String> {
    if let Some(fuzz) = &tx.fuzz {
        for fuzz_param in fuzz {
            if let Some(value) = fuzz_param.value {
                if value {
                    return Some(
                        fuzz_map
                            .get(VALUE_KEY)
                            .expect("value fuzzer was not initialized")[fuzz_idx]
                            .to_string(),
                    );
                }
            }
        }
    }
    None
}
