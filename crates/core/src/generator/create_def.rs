use alloy::{hex::ToHexExt, primitives::Address};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;
use tracing::debug;

use crate::generator::{error::GeneratorError, util::encode_calldata};

#[derive(Clone, Deserialize, Debug, Serialize)]
pub struct CompiledContract<S: AsRef<str> = String> {
    pub bytecode: S,
    pub name: S,
}

pub enum ContractFileType {
    Json {
        /// Relative file path to the file containing bytecode.
        path: PathBuf,
        /// Determines where in the json the bytecode is located. Example: `.bytecode.object` (for forge builds)
        bytecode_filter: String,
    },
    Hex {
        /// Relative file path to the file containing bytecode.
        path: PathBuf,
    },
}

struct MiniJq {
    /// JSON file contents.
    input: serde_json::Value,
}

fn extract_string(
    json: &serde_json::Value,
    filter: &[String],
) -> Result<String, Box<dyn std::error::Error>> {
    if filter.len() == 0 {
        // desired element is in `json`
        let contents: String = json.to_owned().to_string();
        return Ok(contents);
    }

    let key = &filter[0];
    let val = json.get(key);
    if let Some(val) = val {
        // recurse w/ first element of filter removed
        let new_filter = &filter[1..];
        return extract_string(val, new_filter);
    } else {
        return Err("invalid filter, key not found".into());
    }
}

impl MiniJq {
    pub fn new(input: &str) -> Result<Self, serde_json::Error> {
        Ok(Self {
            input: serde_json::from_str(input)?,
        })
    }

    fn path(&self, filter: &str) -> Vec<String> {
        filter.split('.').map(|x| x.to_owned()).collect::<Vec<_>>()
    }

    pub fn value(&self, filter: &str) -> Result<String, Box<dyn std::error::Error>> {
        extract_string(&self.input, &self.path(filter))
    }
}

impl ContractFileType {
    fn read_file(scenario_path: &Path, relative_path: &Path) -> Result<String, ContractError> {
        println!("relative_path: {relative_path:?}");
        std::fs::read_to_string(scenario_path.join(relative_path))
            .map_err(|e| ContractError::ReadFile(e, relative_path.to_owned()))
    }

    pub fn bytecode(&self, scenario_path: &Path) -> Result<String, ContractError> {
        use ContractFileType::*;

        match self {
            Json {
                path,
                bytecode_filter,
            } => {
                println!("wtf: {path:?}");
                // read file
                let file_contents = Self::read_file(scenario_path, path)?;
                // extract bytecode string from json
                MiniJq::new(&file_contents)?
                    .value(&bytecode_filter)
                    .map_err(|_| ContractError::InvalidJsonFilter(bytecode_filter.to_owned()))
            }
            Hex { path } => {
                println!("wtf2: {path:?}");
                Self::read_file(scenario_path, path)
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum ContractError {
    #[error("invalid JSON")]
    JsonParse(#[from] serde_json::Error),

    #[error("invalid contract bytecode configuration: {0}")]
    InvalidConfig(&'static str),

    #[error("failed to read file at {1}: {0}")]
    ReadFile(std::io::Error, PathBuf),

    #[error("'filter' containing location of raw bytecode is required for json files (example: \".bytecode.object\")")]
    FilterRequiredForJson,

    #[error("invalid json filter: \"{0}\"")]
    InvalidJsonFilter(String),

    #[error("unsupported file type, .hex and .json are supported")]
    UnsupportedType,
}

#[derive(Clone, Deserialize, Debug, Serialize)]
/// File spec for contracts.
pub struct ContractFile {
    path: String,
    filter: Option<String>,
}

impl ContractFile {
    pub fn identify(&self) -> Result<ContractFileType, ContractError> {
        use ContractError::*;
        let path = self.path.to_owned();
        if path.ends_with(".hex") {
            return Ok(ContractFileType::Hex { path: path.into() });
        }
        if path.ends_with(".json") {
            if let Some(filter) = self.filter.clone() {
                return Ok(ContractFileType::Json {
                    path: path.into(),
                    bytecode_filter: filter,
                });
            }
            return Err(FilterRequiredForJson);
        }
        return Err(UnsupportedType);
    }
}

#[derive(Clone, Deserialize, Debug, Serialize)]
/// Specification for config files to specify raw bytecode or a file containing raw bytecode.
/// ((Open to changes on the name))
pub struct CompiledContractOrFile<S: AsRef<str> = String> {
    pub bytecode: Option<S>,
    pub bytecode_file: Option<ContractFile>,
    pub name: S,
}

impl<S: AsRef<str>> CompiledContractOrFile<S> {
    fn source(&self) -> Result<ContractSource, ContractError> {
        if self.bytecode.is_some() && self.bytecode_file.is_some() {
            return Err(ContractError::InvalidConfig(
                "cannot specify both 'bytecode' & 'file'",
            ));
        }
        if let Some(bytecode) = &self.bytecode {
            return Ok(ContractSource::Bytecode(bytecode.as_ref().to_string()));
        }
        if let Some(file) = &self.bytecode_file {
            return Ok(ContractSource::File(file.identify()?));
        }
        return Err(ContractError::InvalidConfig(
            "must specify 'bytecode' or 'file' in contract creation",
        ));
    }
}

enum ContractSource {
    Bytecode(String),
    File(ContractFileType),
}

impl CompiledContractOrFile {
    pub fn to_compiled_contract(
        &self,
        scenario_path: PathBuf,
    ) -> Result<CompiledContract, ContractError> {
        Ok(CompiledContract {
            bytecode: match self.source()? {
                ContractSource::Bytecode(bytecode) => bytecode,
                ContractSource::File(file) => file.bytecode(&scenario_path).map(|s| s)?,
            },
            name: self.name.clone(),
        })
    }
}

impl<T: AsRef<str>> CompiledContract<T> {
    pub fn new(bytecode: T, name: T) -> Self {
        CompiledContract { bytecode, name }
    }

    /// Returns the contract name as a template string (wrapped in curly braces).
    pub fn template_name(&self) -> String {
        format!("{{{}}}", self.name.as_ref())
    }
}

impl<'a> From<CompiledContract<&'a str>> for CompiledContract<String> {
    fn from(contract: CompiledContract<&'a str>) -> Self {
        CompiledContract {
            bytecode: contract.bytecode.to_string(),
            name: contract.name.to_string(),
        }
    }
}

impl<'a> From<CompiledContract<&'a str>> for CompiledContractOrFile<String> {
    fn from(value: CompiledContract<&'a str>) -> Self {
        Self {
            bytecode: Some(value.bytecode.to_owned()),
            bytecode_file: None,
            name: value.name.to_owned(),
        }
    }
}

impl<S: AsRef<str> + Clone> From<CompiledContract<S>> for CompiledContractOrFile<S> {
    fn from(value: CompiledContract<S>) -> Self {
        Self {
            bytecode: Some(value.bytecode.clone()),
            bytecode_file: None,
            name: value.name.clone(),
        }
    }
}

impl CompiledContract {
    pub fn with_constructor_args(
        mut self,
        sig: impl AsRef<str>,
        args: &[impl AsRef<str>],
    ) -> Result<Self, GeneratorError> {
        let sig = sig.as_ref();
        let sig = if sig.starts_with("(") {
            // coerce sig into `function(...)` format to make encode_calldata happy
            &format!("constructor{sig}")
        } else {
            sig
        };

        // encode calldata
        let calldata = encode_calldata(args, sig)?;

        // remove function selector from calldata
        let calldata = if calldata.len() >= 4 {
            calldata[4..].to_vec()
        } else {
            Vec::new()
        };
        debug!("calldata (no selector): {}", calldata.encode_hex());

        self.bytecode.push_str(&calldata.encode_hex());

        Ok(self)
    }
}

pub struct CreateDefinitionUnresolved {
    pub contract: CompiledContractOrFile,
}

#[derive(Clone, Deserialize, Debug, Serialize)]
pub struct CreateDefinition {
    #[serde(flatten)]
    pub contract: CompiledContractOrFile,
    /// Constructor signature. Formats supported: "constructor(type1,type2,...)" or "(type1,type2,...)".
    pub signature: Option<String>,
    /// Constructor arguments. May include placeholders.
    pub args: Option<Vec<String>>,
    /// Address of the tx sender.
    pub from: Option<String>,
    /// Get a `from` address from the pool of signers specified here.
    pub from_pool: Option<String>,
}

impl CreateDefinition {
    pub fn new(contract: &CompiledContractOrFile) -> Self {
        CreateDefinition {
            contract: contract.to_owned(),
            signature: None,
            args: None,
            from: None,
            from_pool: None,
        }
    }

    pub fn with_from_pool(mut self, from_pool: impl AsRef<str>) -> Self {
        self.from_pool = Some(from_pool.as_ref().to_owned());
        self
    }

    pub fn with_from(mut self, from: impl AsRef<str>) -> Self {
        self.from = Some(from.as_ref().to_owned());
        self
    }
}

pub struct CreateDefinitionStrict {
    pub bytecode: String,
    pub name: String,
    pub from: Address,
    pub signature: Option<String>,
    pub args: Vec<String>,
}
