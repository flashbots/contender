use crate::{
    db::DbOps,
    error::ContenderError,
    generator::{
        types::{CreateDefinition, FunctionCallDefinition},
        util::encode_calldata,
    },
    Result,
};
use alloy::{
    hex::FromHex,
    primitives::{Address, Bytes, TxKind, U256},
    rpc::types::TransactionRequest,
};
use std::collections::HashMap;

pub trait Templater<K>
where
    K: Eq + std::hash::Hash + ToString + std::fmt::Debug + Send + Sync,
{
    fn replace_placeholders(&self, input: &str, placeholder_map: &HashMap<K, String>) -> String;
    fn terminator_start(&self, input: &str) -> Option<usize>;
    fn terminator_end(&self, input: &str) -> Option<usize>;
    fn copy_end(&self, input: &str, last_end: usize) -> String;
    fn num_placeholders(&self, input: &str) -> usize;
    fn find_key(&self, input: &str) -> Option<(K, usize)>;
    fn encode_contract_address(&self, input: &Address) -> String;

    /// Looks for {placeholders} in `arg` and updates `env` with the values found by querying the DB.
    fn find_placeholder_values(
        &self,
        arg: &str,
        placeholder_map: &mut HashMap<K, String>,
        db: &impl DbOps,
    ) -> Result<()> {
        // count number of placeholders (by left brace) in arg
        let num_template_vals = self.num_placeholders(arg);
        let mut last_end = 0;

        for _ in 0..num_template_vals {
            let template_value = self.copy_end(&arg, last_end);
            let (template_key, template_end) =
                self.find_key(&template_value)
                    .ok_or(ContenderError::SpamError(
                        "failed to find placeholder key",
                        Some(arg.to_string()),
                    ))?;
            last_end = template_end + 1;

            // skip if value in map, else look up in DB
            if placeholder_map.contains_key(&template_key) {
                continue;
            }

            let template_value = db.get_named_tx(&template_key.to_string()).map_err(|e| {
                ContenderError::SpamError(
                    "failed to get placeholder value from DB",
                    Some(format!("value={:?} ({})", template_key, e)),
                )
            })?;
            placeholder_map.insert(
                template_key,
                template_value
                    .address
                    .map(|a| self.encode_contract_address(&a))
                    .unwrap_or_default(),
            );
        }
        Ok(())
    }

    /// Finds {placeholders} in `fncall` and looks them up in `db`,
    /// then inserts the values it finds into `placeholder_map`.
    fn find_fncall_placeholders(
        &self,
        fncall: &FunctionCallDefinition,
        db: &impl DbOps,
        placeholder_map: &mut HashMap<K, String>,
    ) -> Result<()> {
        // find templates in fn args & `to`
        let fn_args = fncall.args.to_owned().unwrap_or_default();
        for arg in fn_args.iter() {
            self.find_placeholder_values(arg, placeholder_map, db)?;
        }
        self.find_placeholder_values(&fncall.to, placeholder_map, db)?;
        Ok(())
    }

    /// Returns a transaction request for a given function call definition with all
    /// {placeholders} filled in using corresponding values from `placeholder_map`.
    fn template_function_call(
        &self,
        funcdef: &FunctionCallDefinition,
        placeholder_map: &HashMap<K, String>,
    ) -> Result<TransactionRequest> {
        let step_args = funcdef.args.to_owned().unwrap_or_default();
        let mut args = Vec::new();
        for arg in step_args.iter() {
            let val = self.replace_placeholders(arg, placeholder_map);
            args.push(val);
        }
        let input = encode_calldata(&args, &funcdef.signature)?;
        let to = self.replace_placeholders(&funcdef.to, placeholder_map);
        let to = to
            .parse::<Address>()
            .map_err(|e| ContenderError::with_err(e, "failed to parse address"))?;
        let value = funcdef
            .value
            .as_ref()
            .map(|s| self.replace_placeholders(s, placeholder_map))
            .map(|s| s.parse::<U256>().ok())
            .flatten();

        let from = funcdef
            .from
            .parse::<Address>()
            .map_err(|e| ContenderError::with_err(e, "failed to parse from address"))?;

        Ok(TransactionRequest {
            to: Some(TxKind::Call(to)),
            input: alloy::rpc::types::TransactionInput::both(input.into()),
            from: Some(from),
            value,
            ..Default::default()
        })
    }

    fn template_contract_deploy(
        &self,
        createdef: &CreateDefinition,
        placeholder_map: &HashMap<K, String>,
    ) -> Result<TransactionRequest> {
        let from = createdef
            .from
            .to_owned()
            .parse::<Address>()
            .map_err(|e| ContenderError::with_err(e, "failed to parse from address"))?;

        let full_bytecode = self.replace_placeholders(&createdef.bytecode, &placeholder_map);
        let tx = alloy::rpc::types::TransactionRequest {
            from: Some(from),
            to: Some(alloy::primitives::TxKind::Create),
            input: alloy::rpc::types::TransactionInput::both(
                Bytes::from_hex(&full_bytecode).expect("invalid bytecode hex"),
            ),
            ..Default::default()
        };
        Ok(tx)
    }
}
