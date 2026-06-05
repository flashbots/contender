use crate::{
    db::{DbError, DbOps},
    generator::{
        constants::{SENDER_KEY, SETCODE_KEY},
        function_def::{FunctionCallDefinition, FunctionCallDefinitionStrict},
        util::{encode_calldata, parse_value, scenario_db_key, UtilError},
        CreateDefinition,
    },
};
use alloy::{
    hex::{FromHex, ToHexExt},
    primitives::{Address, Bytes, FixedBytes, TxKind, U256},
    rpc::types::{AccessList, TransactionRequest},
};
use std::collections::HashMap;
use thiserror::Error;

use super::CreateDefinitionStrict;

pub type Result<T> = std::result::Result<T, TemplaterError>;

#[derive(Debug, Error)]
pub enum TemplaterError {
    #[error("failed to find placeholder key '{0}'")]
    KeyNotFound(String),

    #[error("contract address for '{0}' not found in DB. You may need to run setup steps first.")]
    ContractNotFoundInDB(String),

    #[error("DB error")]
    Db(#[from] DbError),

    #[error("failed to parse address '{0}'")]
    ParseAddressFailed(String),

    #[error("failed to parse max_priority_fee_per_gas '{input}': {reason}")]
    ParsePriorityFeeFailed { input: String, reason: String },

    #[error("templater util error")]
    Util(#[from] UtilError),
}

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
        genesis_hash: FixedBytes<32>,
        scenario_label: Option<&str>,
    ) -> Result<()> {
        // count number of placeholders (by left brace) in arg
        let num_template_vals = self.num_placeholders(arg);
        let mut last_end = 0;
        let mut template_input = arg.to_owned();

        for _ in 0..num_template_vals {
            template_input = self.copy_end(&template_input, last_end);
            let (template_key, template_end) = self
                .find_key(&template_input)
                .ok_or(TemplaterError::KeyNotFound(arg.to_string()))?;
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

            let db_key = scenario_db_key(&template_key, scenario_label);
            let template_value = db
                .get_named_tx(&db_key, rpc_url, genesis_hash)
                .map_err(|e| e.into())?;
            if let Some(template_value) = template_value {
                placeholder_map.insert(
                    template_key,
                    template_value
                        .address
                        .map(|a| a.encode_hex())
                        .unwrap_or_default(),
                );
            } else {
                return Err(TemplaterError::ContractNotFoundInDB(
                    template_key.to_string(),
                ));
            }
        }
        Ok(())
    }

    /// Finds {placeholders} in `fncall` and looks them up in `db`,
    /// then inserts the values it finds into `placeholder_map`.
    /// NOTE: scans `args`, `from`, `to`, `authorization_address`, and `access_list` fields.
    fn find_fncall_placeholders(
        &self,
        fncall: &FunctionCallDefinition,
        db: &impl DbOps,
        placeholder_map: &mut HashMap<K, String>,
        rpc_url: &str,
        genesis_hash: FixedBytes<32>,
        scenario_label: Option<&str>,
    ) -> Result<()> {
        // find templates in fn args & `to`
        let fn_args = fncall.args.to_owned().unwrap_or_default();
        for arg in fn_args.iter() {
            self.find_placeholder_values(
                arg,
                placeholder_map,
                db,
                rpc_url,
                genesis_hash,
                scenario_label,
            )?;
        }
        if let Some(from) = &fncall.from {
            self.find_placeholder_values(
                from,
                placeholder_map,
                db,
                rpc_url,
                genesis_hash,
                scenario_label,
            )?;
        }
        self.find_placeholder_values(
            &fncall.to,
            placeholder_map,
            db,
            rpc_url,
            genesis_hash,
            scenario_label,
        )?;
        if let Some(auth) = &fncall.authorization_address {
            self.find_placeholder_values(
                auth,
                placeholder_map,
                db,
                rpc_url,
                genesis_hash,
                scenario_label,
            )?;
        }
        if let Some(access_list) = &fncall.access_list {
            for item in access_list.iter() {
                self.find_placeholder_values(
                    &item.address,
                    placeholder_map,
                    db,
                    rpc_url,
                    genesis_hash,
                    scenario_label,
                )?;
                for key in &item.storage_keys {
                    self.find_placeholder_values(
                        key,
                        placeholder_map,
                        db,
                        rpc_url,
                        genesis_hash,
                        scenario_label,
                    )?;
                }
            }
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
        genesis_hash: FixedBytes<32>,
        scenario_label: Option<&str>,
    ) -> Result<()> {
        if let Some(args) = &createdef.args {
            for arg in args.iter() {
                self.find_placeholder_values(
                    arg,
                    placeholder_map,
                    db,
                    rpc_url,
                    genesis_hash,
                    scenario_label,
                )?;
            }
        }
        if let Some(from) = &createdef.from {
            self.find_placeholder_values(
                from,
                placeholder_map,
                db,
                rpc_url,
                genesis_hash,
                scenario_label,
            )?;
        }
        // also scan bytecode for placeholders
        self.find_placeholder_values(
            &createdef.contract.bytecode,
            placeholder_map,
            db,
            rpc_url,
            genesis_hash,
            scenario_label,
        )?;
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
            .map_err(|_| TemplaterError::ParseAddressFailed(to))?;
        let value = funcdef
            .value
            .as_ref()
            .map(|x| self.replace_placeholders(x, placeholder_map))
            .and_then(|s| s.parse::<U256>().ok());
        // Accept plain wei ("10000000000"), hex ("0x2540be400"), or unit
        // strings ("10 gwei", "0.001 eth"). The string may also be a
        // `{placeholder}` that resolves to one of those forms. We surface a
        // hard error on a malformed value rather than silently dropping it
        // — a misconfigured fee field used to become `None`, which looked
        // valid in TOML but quietly disabled the override.
        let max_priority_fee_per_gas = funcdef
            .max_priority_fee_per_gas
            .as_ref()
            .map(|raw| {
                let resolved = self.replace_placeholders(raw, placeholder_map);
                let parsed = parse_value(&resolved).map_err(|err| {
                    TemplaterError::ParsePriorityFeeFailed {
                        input: resolved.clone(),
                        reason: err.to_string(),
                    }
                })?;
                u128::try_from(parsed).map_err(|_| TemplaterError::ParsePriorityFeeFailed {
                    input: resolved,
                    reason: "value exceeds u128::MAX".to_owned(),
                })
            })
            .transpose()?;
        let access_list = funcdef.access_list.to_owned().map(AccessList::from);

        Ok(TransactionRequest {
            to: Some(TxKind::Call(to)),
            input: alloy::rpc::types::TransactionInput::both(input.into()),
            from: Some(funcdef.from),
            value,
            gas: funcdef.gas_limit,
            max_priority_fee_per_gas,
            sidecar: funcdef.sidecar.as_ref().map(|sc| sc.to_owned().into()),
            authorization_list: funcdef.authorization.to_owned(),
            access_list,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generator::{function_def::FunctionCallDefinitionStrict, util::complete_tx_request};
    use alloy::consensus::TxType;
    use alloy::primitives::B256;
    use alloy::rpc::types::AccessListItem;

    /// Minimal `Templater<String>` mirroring the production `{key}` syntax,
    /// used so the priority-fee parsing path can be exercised in isolation.
    struct CurlyBraceTemplater;

    impl Templater<String> for CurlyBraceTemplater {
        fn replace_placeholders(
            &self,
            input: &str,
            template_map: &HashMap<String, String>,
        ) -> String {
            let mut output = input.to_owned();
            for (key, value) in template_map.iter() {
                output = output.replace(&format!("{{{key}}}"), value);
            }
            output
        }

        fn terminator_start(&self, input: &str) -> Option<usize> {
            input.find('{')
        }

        fn terminator_end(&self, input: &str) -> Option<usize> {
            input.find('}')
        }

        fn num_placeholders(&self, input: &str) -> usize {
            input.chars().filter(|&c| c == '{').count()
        }

        fn copy_end(&self, input: &str, last_end: usize) -> String {
            input.split_at(last_end).1.to_owned()
        }

        fn find_key(&self, input: &str) -> Option<(String, usize)> {
            let start = self.terminator_start(input)?;
            let end = self.terminator_end(input)?;
            Some((input[start + 1..end].to_owned(), end))
        }
    }

    /// Build a strict call definition whose only meaningful field for the
    /// priority-fee tests is `max_priority_fee_per_gas`. Everything else gets
    /// a uniform default so each test's `#[case]`-like row stays short.
    fn strict_def_with_priority_fee(priority_fee: Option<&str>) -> FunctionCallDefinitionStrict {
        FunctionCallDefinitionStrict {
            to: "0x0000000000000000000000000000000000000001".to_owned(),
            from: Address::ZERO,
            signature: String::new(),
            args: vec![],
            value: None,
            fuzz: vec![],
            kind: None,
            gas_limit: None,
            max_priority_fee_per_gas: priority_fee.map(|s| s.to_owned()),
            sidecar: None,
            authorization: None,
            access_list: None,
        }
    }

    #[test]
    fn priority_fee_accepts_wei_hex_and_unit_strings() {
        // Same expected u128 (10 gwei) produced from three accepted formats,
        // plus a no-fee baseline to confirm `None` round-trips.
        let cases: &[(Option<&str>, Option<u128>)] = &[
            (None, None),
            (Some("10000000000"), Some(10_000_000_000)),
            (Some("0x2540be400"), Some(10_000_000_000)),
            (Some("10 gwei"), Some(10_000_000_000)),
            (Some("0.00000001 eth"), Some(10_000_000_000)),
        ];
        for (input, expected) in cases {
            let templater = CurlyBraceTemplater;
            let strict = strict_def_with_priority_fee(*input);
            let tx = templater
                .template_function_call(&strict, &HashMap::new())
                .unwrap_or_else(|e| panic!("templater must succeed for input={input:?}: {e}"));
            assert_eq!(
                tx.max_priority_fee_per_gas, *expected,
                "priority fee mismatch for input={input:?}",
            );
        }
    }

    #[test]
    fn priority_fee_resolves_placeholder_before_parsing() {
        let templater = CurlyBraceTemplater;
        let strict = strict_def_with_priority_fee(Some("{fee}"));
        let placeholders = HashMap::from([("fee".to_owned(), "10 gwei".to_owned())]);
        let tx = templater
            .template_function_call(&strict, &placeholders)
            .expect("templater must resolve placeholder then parse");
        assert_eq!(tx.max_priority_fee_per_gas, Some(10_000_000_000));
    }

    #[test]
    fn priority_fee_returns_error_for_malformed_string() {
        // Previously these would silently become `None`; now they must error
        // so misconfiguration is visible at scenario-load time.
        for bad in ["banana", "10 banana", "abc gwei"] {
            let templater = CurlyBraceTemplater;
            let strict = strict_def_with_priority_fee(Some(bad));
            let err = templater
                .template_function_call(&strict, &HashMap::new())
                .expect_err(&format!("expected error for input={bad:?}"));
            assert!(
                matches!(err, TemplaterError::ParsePriorityFeeFailed { .. }),
                "wrong error variant for input={bad:?}: {err:?}",
            );
        }
    }

    #[test]
    fn template_function_call_threads_access_list_into_request() {
        let templater = CurlyBraceTemplater;
        let access_list_address = "0x4200000000000000000000000000000000000022";
        let storage_key = "0x0100000000000000000000000000000000000000000000000000000000000000";
        let second_storage_key =
            "0x0300000000000000000000000000000000000000000000000000000000000000";
        let placeholder_map = HashMap::new();
        let funcdef = FunctionCallDefinitionStrict {
            to: access_list_address.to_string(),
            from: Address::ZERO,
            signature: "validate()".to_string(),
            args: vec![],
            value: None,
            fuzz: vec![],
            kind: None,
            gas_limit: Some(200_000),
            max_priority_fee_per_gas: None,
            sidecar: None,
            authorization: None,
            access_list: Some(vec![AccessListItem {
                address: access_list_address.parse::<Address>().unwrap(),
                storage_keys: vec![
                    storage_key.parse::<B256>().unwrap(),
                    second_storage_key.parse::<B256>().unwrap(),
                ],
            }]),
        };

        let mut tx = templater
            .template_function_call(&funcdef, &placeholder_map)
            .unwrap();
        let access_list = tx.access_list.as_ref().unwrap();

        assert_eq!(access_list.len(), 1);
        assert_eq!(
            access_list[0].address,
            access_list_address.parse::<Address>().unwrap()
        );
        assert_eq!(access_list[0].storage_keys.len(), 2);
        assert_eq!(
            access_list[0].storage_keys[0],
            storage_key.parse::<B256>().unwrap()
        );
        assert_eq!(
            access_list[0].storage_keys[1],
            second_storage_key.parse::<B256>().unwrap()
        );

        complete_tx_request(&mut tx, TxType::Eip1559, 10, 1, 200_000, 1, 0);

        assert_eq!(tx.access_list.unwrap().len(), 1);
        assert_eq!(tx.max_fee_per_gas, Some(10));
        assert_eq!(tx.max_priority_fee_per_gas, Some(1));
        assert_eq!(tx.chain_id, Some(1));
    }
}
