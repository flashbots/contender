use super::{
    types::{CreateDefinition, FunctionCallDefinition, FuzzParam},
    NamedTxRequest,
};
use crate::{
    db::database::DbOps,
    error::ContenderError,
    generator::{
        seeder::{SeedValue, Seeder},
        templater::Templater,
    },
    Result,
};
use alloy::primitives::U256;
use async_trait::async_trait;
use std::{collections::HashMap, fmt::Debug, hash::Hash};
use tokio::task::JoinHandle;

pub trait PlanConfig<K>
where
    K: Eq + Hash + Debug + Send + Sync,
{
    fn get_env(&self) -> Result<HashMap<K, String>>;
    fn get_create_steps(&self) -> Result<Vec<CreateDefinition>>;
    fn get_setup_steps(&self) -> Result<Vec<FunctionCallDefinition>>;
    fn get_spam_steps(&self) -> Result<Vec<FunctionCallDefinition>>;
}

#[derive(Debug)]
pub struct Plan {
    pub env: HashMap<String, String>,
    pub create_steps: Vec<NamedTxRequest>,
    pub setup_steps: Vec<NamedTxRequest>,
    pub spam_steps: Vec<NamedTxRequest>,
}

type CallbackResult = Result<Option<JoinHandle<()>>>;

pub enum PlanType<F: Fn(&NamedTxRequest) -> CallbackResult> {
    Create(F),
    Setup(F),
    Spam(usize, F),
}

#[async_trait]
pub trait Generator2<K, D, T>
where
    K: Eq + Hash + Debug + ToString + ToOwned<Owned = K> + Send + Sync,
    D: Send + Sync + DbOps,
    T: Send + Sync + Templater<K>,
{
    fn get_plan_conf(&self) -> &impl PlanConfig<K>;
    fn get_templater(&self) -> &T;
    fn get_db(&self) -> &D;
    fn get_fuzz_seeder(&self) -> &impl Seeder;

    fn get_fuzz_map(
        &self,
        num_values: usize,
        fuzz_args: &[FuzzParam],
    ) -> HashMap<String, Vec<U256>> {
        let mut fuzz_map = HashMap::<String, Vec<U256>>::new();
        let seed = self.get_fuzz_seeder();
        for fuzz in fuzz_args {
            let values: Vec<U256> = seed
                .seed_values(num_values, fuzz.min, fuzz.max)
                .map(|v| v.as_u256())
                .collect();
            fuzz_map.insert(fuzz.param.to_owned(), values);
        }
        fuzz_map
    }

    async fn load_txs<F: Send + Sync + Fn(&NamedTxRequest) -> CallbackResult>(
        &self,
        plan_type: PlanType<F>,
    ) -> Result<Vec<NamedTxRequest>> {
        let conf = self.get_plan_conf();
        let env = conf.get_env()?;
        let db = self.get_db();
        let templater = self.get_templater();

        let mut placeholder_map = HashMap::<K, String>::new();
        for (key, value) in env.iter() {
            placeholder_map.insert(key.to_owned(), value.to_owned());
        }

        let mut txs = vec![];
        match plan_type {
            PlanType::Create(on_create_step) => {
                let create_steps = conf.get_create_steps()?;
                for step in create_steps.iter() {
                    // lookup placeholder values in DB & update map before templating
                    templater.find_placeholder_values(&step.bytecode, &mut placeholder_map, db)?;

                    // create txs with template values
                    let tx = NamedTxRequest {
                        name: Some(step.name.to_owned()),
                        tx: templater.template_contract_deploy(step, &placeholder_map)?,
                    };
                    let handle = on_create_step(&tx)?;
                    if let Some(handle) = handle {
                        handle.await.map_err(|e| {
                            ContenderError::with_err(e, "join error; callback crashed")
                        })?;
                    }
                    txs.push(tx);
                }
            }
            PlanType::Setup(on_setup_step) => {
                let setup_steps = conf.get_setup_steps()?;
                for step in setup_steps.iter() {
                    // lookup placeholders in DB & update map before templating
                    templater.find_fncall_placeholders(step, db, &mut placeholder_map)?;

                    // create txs with template values
                    let tx = templater
                        .template_function_call(step, &placeholder_map)?
                        .into();
                    let handle = on_setup_step(&tx)?;
                    if let Some(handle) = handle {
                        handle.await.map_err(|e| {
                            ContenderError::with_err(e, "join error; callback crashed")
                        })?;
                    }
                    txs.push(tx);
                }
            }
            PlanType::Spam(num_txs, on_spam_setup) => {
                let spam_steps = conf.get_spam_steps()?;
                let num_steps = spam_steps.len();
                // round num_txs up to the nearest multiple of num_steps to prevent missed steps
                let num_txs = num_txs + (num_txs % num_steps);

                for step in spam_steps.iter() {
                    // find templates from fn call
                    templater.find_fncall_placeholders(step, db, &mut placeholder_map)?;
                    let fn_args = step.args.to_owned().unwrap_or_default();

                    // parse fn signature, used to check for fuzzed args later (to make sure they're named)
                    let func = alloy::json_abi::Function::parse(&step.signature).map_err(|e| {
                        ContenderError::with_err(e, "failed to parse function name")
                    })?;

                    // pre-generate fuzzy values for each fuzzed parameter
                    let fuzz_args = step.fuzz.to_owned().unwrap_or(vec![]);
                    let fuzz_map = self.get_fuzz_map(num_txs / num_steps, &fuzz_args);

                    // generate spam txs; split total amount by number of spam steps
                    for i in 0..(num_txs / num_steps) {
                        // check args for fuzz params first
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
                                if fuzz_map.contains_key(arg_name) {
                                    return Some(fuzz_map.get(arg_name).unwrap()[i].to_string());
                                }
                                None
                            };

                            // !!! args with template values will be overwritten by the fuzzer if it's enabled for this arg
                            let val = maybe_fuzz().unwrap_or(fn_args[j].to_owned());
                            args.push(val);
                        }
                        let mut step = step.to_owned();
                        step.args = Some(args);

                        let tx: NamedTxRequest = templater
                            .template_function_call(&step, &placeholder_map)?
                            .into();
                        let handle = on_spam_setup(&tx.to_owned())?;
                        if let Some(handle) = handle {
                            handle
                                .await
                                .map_err(|e| ContenderError::with_err(e, "error from callback"))?;
                        }
                        txs.push(tx);
                    }
                }
            }
        }

        Ok(txs)
    }
}
