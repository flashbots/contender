use crate::{
    db::{DbError, DbOps},
    generator::{
        constants::{SENDER_KEY, SETCODE_KEY},
        function_def::{
            AccessListDefinition, FunctionCallDefinition, FunctionCallDefinitionStrict,
        },
        util::{encode_calldata, scenario_db_key, UtilError},
        CreateDefinition,
    },
};
use alloy::{
    eips::eip2930::{AccessList, AccessListItem},
    hex::{FromHex, ToHexExt},
    primitives::{Address, Bytes, FixedBytes, TxKind, B256, U256},
    rpc::types::TransactionRequest,
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

    #[error("failed to parse storage key '{0}'")]
    ParseStorageKeyFailed(String),

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
    /// NOTE: only finds placeholders in `args`, `authorization_addr`, `access_list`, and `to` fields.
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
            for entry in access_list {
                self.find_placeholder_values(
                    &entry.address,
                    placeholder_map,
                    db,
                    rpc_url,
                    genesis_hash,
                    scenario_label,
                )?;
                for storage_key in &entry.storage_keys {
                    self.find_placeholder_values(
                        storage_key,
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

    fn template_access_list(
        &self,
        access_list: &[AccessListDefinition],
        placeholder_map: &HashMap<K, String>,
    ) -> Result<AccessList> {
        access_list
            .iter()
            .map(|entry| {
                let address = self.replace_placeholders(&entry.address, placeholder_map);
                let address = address
                    .parse::<Address>()
                    .map_err(|_| TemplaterError::ParseAddressFailed(address))?;
                let storage_keys = entry
                    .storage_keys
                    .iter()
                    .map(|storage_key| {
                        let storage_key = self.replace_placeholders(storage_key, placeholder_map);
                        storage_key
                            .parse::<B256>()
                            .map_err(|_| TemplaterError::ParseStorageKeyFailed(storage_key))
                    })
                    .collect::<Result<Vec<_>>>()?;

                Ok(AccessListItem {
                    address,
                    storage_keys,
                })
            })
            .collect::<Result<Vec<_>>>()
            .map(AccessList::from)
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
        let access_list = funcdef
            .access_list
            .as_deref()
            .map(|list| self.template_access_list(list, placeholder_map))
            .transpose()?;

        Ok(TransactionRequest {
            to: Some(TxKind::Call(to)),
            input: alloy::rpc::types::TransactionInput::both(input.into()),
            from: Some(funcdef.from),
            value,
            gas: funcdef.gas_limit,
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
    use crate::generator::{
        function_def::{AccessListDefinition, FunctionCallDefinitionStrict},
        util::complete_tx_request,
    };
    use alloy::consensus::TxType;
    use std::collections::HashMap;

    struct TestTemplater;

    impl Templater<String> for TestTemplater {
        fn replace_placeholders(
            &self,
            input: &str,
            placeholder_map: &HashMap<String, String>,
        ) -> String {
            let mut output = input.to_owned();
            for (key, value) in placeholder_map {
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

        fn copy_end(&self, input: &str, last_end: usize) -> String {
            input[last_end..].to_string()
        }

        fn num_placeholders(&self, input: &str) -> usize {
            input.matches('{').count()
        }

        fn find_key(&self, input: &str) -> Option<(String, usize)> {
            let start = self.terminator_start(input)?;
            let end = self.terminator_end(input)?;
            Some((input[start + 1..end].to_string(), end))
        }
    }

    #[test]
    fn template_function_call_threads_access_list_into_request() {
        let templater = TestTemplater;
        let access_list_address = "0x4200000000000000000000000000000000000022";
        let storage_key = "0x0100000000000000000000000000000000000000000000000000000000000000";
        let placeholder_storage_key =
            "0x0300000000000000000000000000000000000000000000000000000000000000";
        let placeholder_map = HashMap::from([(
            "lookup_key".to_string(),
            placeholder_storage_key.to_string(),
        )]);
        let funcdef = FunctionCallDefinitionStrict {
            to: access_list_address.to_string(),
            from: Address::ZERO,
            signature: "validate()".to_string(),
            args: vec![],
            value: None,
            fuzz: vec![],
            kind: None,
            gas_limit: Some(200_000),
            sidecar: None,
            authorization: None,
            access_list: Some(vec![AccessListDefinition {
                address: access_list_address.to_string(),
                storage_keys: vec![storage_key.to_string(), "{lookup_key}".to_string()],
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
            placeholder_storage_key.parse::<B256>().unwrap()
        );

        complete_tx_request(&mut tx, TxType::Eip1559, 10, 1, 200_000, 1, 0);

        assert_eq!(tx.access_list.unwrap().len(), 1);
        assert_eq!(tx.max_fee_per_gas, Some(10));
        assert_eq!(tx.max_priority_fee_per_gas, Some(1));
        assert_eq!(tx.chain_id, Some(1));
    }
}
