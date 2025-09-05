use crate::{
    db::DbOps,
    error::ContenderError,
    generator::{
        constants::{SENDER_KEY, SETCODE_KEY},
        function_def::{FunctionCallDefinition, FunctionCallDefinitionStrict},
        util::encode_calldata,
        CreateDefinition,
    },
    Result,
};
use alloy::{
    hex::{FromHex, ToHexExt},
    primitives::{Address, Bytes, TxKind, U256},
    rpc::types::TransactionRequest,
};
use std::collections::HashMap;

use super::CreateDefinitionStrict;

pub trait Templater<K>
where
    K: Eq + std::hash::Hash + ToString + std::fmt::Debug + Send + Sync,
{
    /// Searches input for {placeholders} and replaces them, then returns the formatted string containing the new value injected into the placeholder.
    fn replace_placeholders(&self, input: &str, placeholder_map: &HashMap<K, String>) -> String;
    fn terminator_start(&self, input: &str) -> Option<usize>;
    fn terminator_end(&self, input: &str) -> Option<usize>;
    fn copy_end(&self, input: &str, last_end: usize) -> String;
    fn num_placeholders(&self, input: &str) -> usize;
    fn find_key(&self, input: &str) -> Option<(K, usize)>;

    /// Looks for {placeholders} in `arg` and updates `env` with the values found by querying the DB.
    fn find_placeholder_values(
        &self,
        arg: &str,
        placeholder_map: &mut HashMap<K, String>,
        db: &impl DbOps,
        rpc_url: &str,
    ) -> Result<()> {
        // count number of placeholders (by left brace) in arg
        let num_template_vals = self.num_placeholders(arg);
        let mut last_end = 0;
        let mut template_input = arg.to_owned();

        for _ in 0..num_template_vals {
            template_input = self.copy_end(&template_input, last_end);
            let (template_key, template_end) =
                self.find_key(&template_input)
                    .ok_or(ContenderError::SpamError(
                        "failed to find placeholder key",
                        Some(arg.to_string()),
                    ))?;
            last_end = template_end + 1;

            // ignore {_sender} placeholder; it's handled outside the templater
            let key = template_key.to_string();
            if key == SENDER_KEY || key == SETCODE_KEY {
                continue;
            }

            // skip if value in map, else look up in DB
            if placeholder_map.contains_key(&template_key) {
                continue;
            }

            let template_value = db
                .get_named_tx(&template_key.to_string(), rpc_url)
                .map_err(|e| {
                    ContenderError::SpamError(
                        "Failed to get named tx from DB. There may be an issue with your database.",
                        Some(format!("value={template_key:?} ({e})")),
                    )
                })?;
            if let Some(template_value) = template_value {
                placeholder_map.insert(
                    template_key,
                    template_value
                        .address
                        .map(|a| a.encode_hex())
                        .unwrap_or_default(),
                );
            } else {
                return Err(ContenderError::SpamError(
                    "Address for named contract not found in DB. You may need to run setup steps first.",
                    Some(template_key.to_string()),
                ));
            }
        }
        Ok(())
    }

    /// Finds {placeholders} in `fncall` and looks them up in `db`,
    /// then inserts the values it finds into `placeholder_map`.
    /// NOTE: only finds placeholders in `args`, `authorization_addr`, and `to` fields.
    fn find_fncall_placeholders(
        &self,
        fncall: &FunctionCallDefinition,
        db: &impl DbOps,
        placeholder_map: &mut HashMap<K, String>,
        rpc_url: &str,
    ) -> Result<()> {
        // find templates in fn args & `to`
        let fn_args = fncall.args.to_owned().unwrap_or_default();
        for arg in fn_args.iter() {
            self.find_placeholder_values(arg, placeholder_map, db, rpc_url)?;
        }
        if let Some(from) = &fncall.from {
            self.find_placeholder_values(from, placeholder_map, db, rpc_url)?;
        }
        self.find_placeholder_values(&fncall.to, placeholder_map, db, rpc_url)?;
        if let Some(auth) = &fncall.authorization_address {
            self.find_placeholder_values(auth, placeholder_map, db, rpc_url)?;
        }
        Ok(())
    }

    /// Finds {placeholders} in create constructor args and updates the placeholder map.
    fn find_create_placeholders(
        &self,
        createdef: &CreateDefinition,
        db: &impl DbOps,
        placeholder_map: &mut HashMap<K, String>,
        rpc_url: &str,
    ) -> Result<()> {
        if let Some(args) = &createdef.args {
            for arg in args.iter() {
                self.find_placeholder_values(arg, placeholder_map, db, rpc_url)?;
            }
        }
        if let Some(from) = &createdef.from {
            self.find_placeholder_values(from, placeholder_map, db, rpc_url)?;
        }
        // also scan bytecode for placeholders
        self.find_placeholder_values(&createdef.contract.bytecode, placeholder_map, db, rpc_url)?;
        Ok(())
    }

    /// Returns a transaction request for a given function call definition with all
    /// {placeholders} filled in using corresponding values from `placeholder_map`.
    fn template_function_call(
        &self,
        funcdef: &FunctionCallDefinitionStrict,
        placeholder_map: &HashMap<K, String>,
    ) -> Result<TransactionRequest> {
        let mut args = Vec::new();

        for arg in funcdef.args.iter() {
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
            .and_then(|s| s.parse::<U256>().ok());

        Ok(TransactionRequest {
            to: Some(TxKind::Call(to)),
            input: alloy::rpc::types::TransactionInput::both(input.into()),
            from: Some(funcdef.from),
            value,
            gas: funcdef.gas_limit,
            sidecar: funcdef.sidecar.to_owned(),
            authorization_list: funcdef.authorization.to_owned(),
            ..Default::default()
        })
    }

    fn template_contract_deploy(
        &self,
        createdef: &CreateDefinitionStrict,
        placeholder_map: &HashMap<K, String>,
    ) -> Result<TransactionRequest> {
        let mut full_bytecode = self.replace_placeholders(&createdef.bytecode, placeholder_map);

        // If a constructor signature is provided, encode args and append to bytecode
        if let Some(sig) = &createdef.signature {
            let mut args = Vec::new();
            for arg in &createdef.args {
                if arg == "{_sender}" {
                    args.push(createdef.from.to_string());
                } else {
                    args.push(self.replace_placeholders(arg, placeholder_map));
                }
            }

            // support both "constructor(type,...)" and "(type,...)" inputs
            let sig = if sig.starts_with("(") {
                format!("constructor{}", sig)
            } else {
                sig.to_owned()
            };

            let mut calldata = encode_calldata(&args, &sig)?;
            // strip 4-byte selector
            if calldata.len() >= 4 {
                calldata = calldata[4..].to_vec();
            } else {
                calldata = Vec::new();
            }
            // append hex-encoded constructor calldata to bytecode
            full_bytecode.push_str(&calldata.encode_hex());
        }

        let tx = alloy::rpc::types::TransactionRequest {
            from: Some(createdef.from),
            to: Some(alloy::primitives::TxKind::Create),
            input: alloy::rpc::types::TransactionInput::both(
                Bytes::from_hex(&full_bytecode).expect("invalid bytecode hex"),
            ),
            ..Default::default()
        };
        Ok(tx)
    }
}
