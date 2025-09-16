use crate::{
    agent_controller::{AgentStore, SignerRegistry},
    db::DbOps,
    error::ContenderError,
    generator::{
        constants::*,
        function_def::{FunctionCallDefinition, FunctionCallDefinitionStrict, FuzzParam},
        named_txs::{ExecutionRequest, NamedTxRequest, NamedTxRequestBuilder},
        seeder::{SeedValue, Seeder},
        templater::Templater,
        types::{AnyProvider, CallbackResult, PlanType, SpamRequest},
        CreateDefinition, CreateDefinitionStrict,
    },
    Result,
};
use alloy::{
    consensus::{SidecarBuilder, SimpleCoder},
    eips::eip7702::SignedAuthorization,
    hex::{FromHex, ToHexExt},
    primitives::{Address, Bytes, FixedBytes, U256},
    rpc::types::Authorization,
    signers::{local::PrivateKeySigner, SignerSync},
};
use async_trait::async_trait;
use std::{collections::HashMap, fmt::Debug, hash::Hash};

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

    /// Returns unique from_pool declarations from the `create` section of the testfile.
    fn get_create_pools(&self) -> Vec<String> {
        let mut from_pools: Vec<_> = self
            .get_create_steps()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|s| s.from_pool)
            .collect();
        from_pools.sort();
        from_pools.dedup();
        from_pools
    }

    /// Returns unique from_pool declarations from the `setup` section of the testfile.
    fn get_setup_pools(&self) -> Vec<String> {
        let mut from_pools: Vec<_> = self
            .get_setup_steps()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|s| s.from_pool)
            .collect();
        from_pools.sort();
        from_pools.dedup();
        from_pools
    }

    /// Returns unique from_pool declarations from the `spam` section of the testfile.
    fn get_spam_pools(&self) -> Vec<String> {
        let mut from_pools = vec![];
        let spam = self
            .get_spam_steps()
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
    fn get_chain_id(&self) -> u64;
    fn get_rpc_provider(&self) -> &AnyProvider;
    fn get_nonce_map(&self) -> &HashMap<Address, u64>;
    fn get_setcode_signer(&self) -> &PrivateKeySigner;
    fn get_genesis_hash(&self) -> FixedBytes<32>;

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
            agent
                .get_address(idx % agent.signers.len())
                .ok_or(ContenderError::SpamError(
                    "signer not found in agent store",
                    Some(format!("from_pool={from_pool}, idx={idx}")),
                ))?
        } else if let Some(from) = &create_def.from {
            // inject env vars into placeholders where/if present
            let placeholder_map = self.get_plan_conf().get_env()?;
            let from_address = self
                .get_templater()
                .replace_placeholders(from, &placeholder_map);
            from_address.parse().map_err(|e| {
                ContenderError::SpamError(
                    "failed to parse 'from' address",
                    Some(format!("from={from}, error={e}")),
                )
            })?
        } else {
            return Err(ContenderError::SpamError(
                "invalid runtime config: must specify 'from' or 'from_pool'",
                None,
            ));
        };

        // handle direct variable injection
        // (backwards-compatible for bytecode defs that include placeholders,
        // rather than using `args` + `signature` in the `CreateDefinition`)
        let bytecode = create_def.contract.bytecode.to_owned().replace(
            "{_sender}",
            &format!("{}{}", "0".repeat(24), from_address.encode_hex()),
        ); // inject address WITHOUT 0x prefix, padded with 24 zeroes

        Ok(CreateDefinitionStrict {
            name: create_def.contract.name.to_owned(),
            bytecode,
            from: from_address,
            signature: create_def.signature.to_owned(),
            args: create_def.args.to_owned().unwrap_or_default(),
        })
    }

    /// Converts a `FunctionCallDefinition` to a `FunctionCallDefinitionStrict`, replacing
    /// `{_sender}` with the `from` address, and ensuring that the `from` address is valid.
    /// If `from_pool` is specified, it will use the address of the signer at index `idx`.
    /// If `from` is specified, it will parse the address from the string.
    /// If neither is specified, it will return an error.
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
            let signer =
                agent
                    .get_signer(idx % agent.signers.len())
                    .ok_or(ContenderError::SpamError(
                        "signer not found in agent store",
                        Some(format!("from_pool={from_pool}, idx={idx}")),
                    ))?;
            signer.address()
        } else if let Some(from) = &funcdef.from {
            // inject env vars into placeholders where/if present
            let placeholder_map = self.get_plan_conf().get_env()?;
            let from_address = self
                .get_templater()
                .replace_placeholders(from, &placeholder_map);
            from_address.parse::<Address>().map_err(|e| {
                ContenderError::SpamError(
                    "failed to parse 'from' address",
                    Some(format!("from={from}, error={e}")),
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

        // replace special variables with the corresponding special values
        let special_replace = |arg: &String| {
            if arg.contains(&sender_placeholder()) {
                // return `from` address WITH 0x prefix
                arg.replace(&sender_placeholder(), &from_address.to_string())
            } else if arg.contains(&setcode_placeholder()) {
                arg.replace(
                    &setcode_placeholder(),
                    &self.get_setcode_signer().address().to_string(),
                )
            } else {
                arg.to_owned()
            }
        };
        let args = args.iter().map(special_replace).collect::<Vec<String>>();
        let to_address = special_replace(&funcdef.to);

        let sidecar_data = if let Some(data) = funcdef.blob_data.as_ref() {
            let parsed_data = Bytes::from_hex(if data.starts_with("0x") {
                data.to_owned()
            } else {
                data.encode_hex()
            })
            .map_err(|e| {
                ContenderError::with_err(e, "failed to parse blob data; invalid hex value")
            })?;
            let sidecar = SidecarBuilder::<SimpleCoder>::from_slice(&parsed_data)
                .build()
                .map_err(|e| ContenderError::with_err(e, "failed to build sidecar"))?;
            Some(sidecar)
        } else {
            None
        };

        let signed_auth = if let Some(auth_address) = &funcdef.authorization_address {
            let mut placeholder_map = HashMap::<K, String>::new();
            let templater = self.get_templater();
            templater.find_fncall_placeholders(
                funcdef,
                self.get_db(),
                &mut placeholder_map,
                &self.get_rpc_url(),
                self.get_genesis_hash(),
            )?;
            // contract address; we'll copy its code to our EOA
            let actual_auth_address = self
                .get_templater()
                .replace_placeholders(auth_address, &placeholder_map)
                .parse::<Address>()
                .map_err(|e| {
                    ContenderError::with_err(e, "failed to find address in placeholder map")
                })?;
            let setcode_signer = self.get_setcode_signer();

            // the setcode nonce won't be updated in time for this function to recognize it
            // so we get the latest nonce (available from init) and add `idx`
            let setcode_nonce = self.get_nonce_map().get(&setcode_signer.address()).ok_or(
                ContenderError::GenericError(
                    "failed to find nonce for address:",
                    format!("{}", setcode_signer.address()),
                ),
            )? + idx as u64;

            // build & sign EIP-7702 authorization
            let auth_req = Authorization {
                address: actual_auth_address,
                chain_id: U256::from(self.get_chain_id()),
                nonce: setcode_nonce,
            };
            Some(sign_auth(setcode_signer, auth_req)?)
        } else {
            None
        };

        Ok(FunctionCallDefinitionStrict {
            to: to_address,
            from: from_address,
            signature: funcdef.signature.to_owned().unwrap_or_default(),
            args,
            value: funcdef.value.to_owned(),
            fuzz: funcdef.fuzz.to_owned().unwrap_or_default(),
            kind: funcdef.kind.to_owned(),
            gas_limit: funcdef.gas_limit.to_owned(),
            sidecar: sidecar_data,
            authorization: signed_auth.map(|a| vec![a]),
        })
    }

    /// Loads transactions from the plan configuration and returns execution requests.
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

                for step in create_steps.iter() {
                    // lookup placeholder values in DB & update map before templating (bytecode + args)
                    templater.find_create_placeholders(
                        step,
                        db,
                        &mut placeholder_map,
                        &self.get_rpc_url(),
                        self.get_genesis_hash(),
                    )?;

                    // populate step with from address
                    let step = self.make_strict_create(step, 0)?;

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
                        })??;
                    }
                    txs.push(tx.into());
                }
            }
            PlanType::Setup(on_setup_step) => {
                let setup_steps = conf.get_setup_steps()?;
                let rpc_url = self.get_rpc_url();

                for step in setup_steps.iter() {
                    // lookup placeholders in DB & update map before templating
                    templater.find_fncall_placeholders(
                        step,
                        db,
                        &mut placeholder_map,
                        &rpc_url,
                        self.get_genesis_hash(),
                    )?;

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
                        })??;
                    }
                    txs.push(tx.into());
                }
            }
            PlanType::Spam(num_txs, on_spam_setup) => {
                let spam_steps = conf.get_spam_steps()?;
                let num_steps = spam_steps.len() as u64;
                // round num_txs up to the nearest multiple of num_steps to prevent missed steps
                let num_txs = num_txs + (num_txs % num_steps);
                let mut canonical_fuzz_map = HashMap::<String, Vec<U256>>::new();

                // finds fuzzed values for a function call definition and populates `canonical_fuzz_map` with fuzzy values.
                let mut find_fuzz = |req: &FunctionCallDefinition| {
                    let fuzz_args = req.fuzz.to_owned().unwrap_or_default();
                    let fuzz_map = self.create_fuzz_map(num_txs as usize, &fuzz_args)?; // this may create more values than needed, but it's fine
                    canonical_fuzz_map.extend(fuzz_map);
                    Ok::<_, ContenderError>(())
                };

                // finds placeholders in a function call definition and populates `placeholder_map` and `canonical_fuzz_map` with injectable values.
                let rpc_url = self.get_rpc_url();
                let mut lookup_tx_placeholders = |tx: &FunctionCallDefinition| {
                    let res = templater.find_fncall_placeholders(
                        tx,
                        db,
                        &mut placeholder_map,
                        &rpc_url,
                        self.get_genesis_hash(),
                    );
                    if let Err(e) = res {
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

                // txs will be grouped by full step sequences [from=1, from=2, from=3, from=1, from=2, from=3, ...]
                for step in spam_steps.iter() {
                    for i in 0..(num_txs / num_steps) as usize {
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
                                    &self.make_strict_call(&req, i)?, // 'from' address injected here
                                    &placeholder_map,
                                )?,
                                None,
                                req.kind.to_owned(),
                            );
                            let setup_res = on_spam_setup(tx.to_owned())?;
                            Ok::<_, ContenderError>((setup_res, tx))
                        };

                        match step {
                            SpamRequest::Tx(req) => {
                                let (handle, tx) = prepare_tx(req)?;
                                if let Some(handle) = handle {
                                    handle.await.map_err(|e| {
                                        ContenderError::with_err(e, "error from callback")
                                    })??;
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
                                        })??;
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
    if let Some(tx_signature) = &tx.signature {
        let func = alloy::json_abi::Function::parse(tx_signature)
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
    } else {
        vec![]
    }
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

pub fn sign_auth(signer: &PrivateKeySigner, auth: Authorization) -> Result<SignedAuthorization> {
    let auth_sig = signer
        .sign_hash_sync(&auth.signature_hash())
        .map_err(|e| ContenderError::with_err(e, "failed to sign authorization hash"))?;
    Ok(auth.into_signed(auth_sig))
}
