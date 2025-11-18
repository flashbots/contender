use crate::default_scenarios::builtin::ToTestConfig;
use crate::util::bold;
use alloy::json_abi::JsonAbi;
use contender_core::generator::error::GeneratorError;
use contender_core::generator::types::SpamRequest;
use contender_core::generator::util::encode_calldata;
use contender_core::generator::{CompiledContract, CreateDefinition, FunctionCallDefinition};
use contender_testfile::TestConfig;
use std::process::Command;
use thiserror::Error;
use tracing::debug;

const ARTIFACTS_PATH: &str = "/tmp/contender-contracts";

#[derive(Clone, Debug, clap::Parser)]
pub struct CustomContractCliArgs {
    /// Path to smart contract source. Format: <path/to/contract.sol>:<ContractName>
    contract_path: std::path::PathBuf,

    #[arg(
        long,
        short,
        visible_aliases = ["ca"],
        long_help = "Comma-separated constructor arguments. Format: \"arg1, arg2, ...\" ",
    )]
    constructor_args: Option<String>,

    #[arg(
        long = "setup",
        num_args = 1,
        action = clap::ArgAction::Append,
        help = "Setup function calls that run once before spamming. May be specified multiple times. Format: \"functionName(...args)\"",
        long_help = "Setup function calls that run once before spamming. May be specified multiple times. Example: `--spam \"setNumber(123456)\"`"
    )]
    setup_calls: Vec<String>,

    #[arg(
        long = "spam",
        num_args = 1,
        action = clap::ArgAction::Append,
        help = "Spam function calls. May be specified multiple times. Format: \"functionName(...args)\"",
        long_help = "Spam function calls. May be specified multiple times. Example: `--spam \"setNumber(123456)\"`"
    )]
    spam_calls: Vec<String>,
}

impl CustomContractCliArgs {
    pub fn constructor_args(&self) -> Vec<String> {
        if let Some(constructor_args) = &self.constructor_args {
            return constructor_args
                .split(',')
                .map(|arg| arg.trim().to_string())
                .filter(|arg| !arg.is_empty())
                .collect::<Vec<String>>();
        }
        vec![]
    }

    /// read smart contract src.
    /// format must be <path/to/contract.sol>:<ContractName>
    pub fn read_sol_file(&self) -> Result<ContractMeta, CustomContractArgsError> {
        let contract_path = self.contract_path.to_str().expect("contract"); // unwrap bc we already read this from a string

        let re = regex::Regex::new(r"^(.+\.sol):([A-Za-z_][A-Za-z0-9_]*)$")?;
        let caps = re.captures(contract_path).ok_or(
            CustomContractArgsError::InvalidContractPathFormat(contract_path.to_owned()),
        )?;
        let contract_path = caps
            .get(1)
            .ok_or(CustomContractArgsError::InvalidContractPath(
                contract_path.to_owned(),
            ))?
            .as_str();
        let contract_name = caps
            .get(2)
            .ok_or(CustomContractArgsError::InvalidContractName(
                contract_path.to_owned(),
            ))?
            .as_str();

        let contract_filename = std::path::Path::new(contract_path)
            .file_name()
            .and_then(|f| f.to_str())
            .ok_or_else(|| CustomContractArgsError::FilenameNotFound(contract_path.to_owned()))?;

        let contract_path_obj = std::path::Path::new(contract_path);
        let parent_dir =
            contract_path_obj
                .parent()
                .ok_or(CustomContractArgsError::ParentDirectoryNotFound(
                    contract_path.to_owned(),
                ))?;

        // try to find root directory containing foundry.toml, otherwise assume the source file's parent dir
        let mut root_dir = parent_dir;
        if let Some(foundry_dir) = find_foundry_toml(parent_dir) {
            root_dir = foundry_dir;
        }
        let root_dir = root_dir.to_str().expect("directory is well-formed");

        Ok(ContractMeta {
            name: contract_name.to_owned(),
            filename: contract_filename.to_owned(),
            root_dir: root_dir.to_owned(),
        })
    }
}

#[derive(Debug, Error)]
pub enum CustomContractArgsError {
    #[error("regex error")]
    Regex(#[from] regex::Error),

    #[error(
        "invalid contract path format ('{0}'); expected <path/to/contract.sol>:<ContractName>"
    )]
    InvalidContractPathFormat(String),

    #[error("invalid contract path: {0}")]
    InvalidContractPath(String),

    #[error("invalid name in contract path: {0}")]
    InvalidContractName(String),

    #[error("path {0} does not include a filename")]
    FilenameNotFound(String),

    #[error("failed to access parent directory of path {0}")]
    ParentDirectoryNotFound(String),

    #[error(
        "invalid CLI params: must provide at least one {} argument",
        bold("--spam")
    )]
    SpamArgsEmpty,

    #[error("core error")]
    Core(#[from] contender_core::Error),

    #[error("contract error")]
    Meta(#[from] ContractMetaError),

    #[error("generator error")]
    Generator(#[from] GeneratorError),
}

/// This contract is expected to have its constructor args already appended to the bytecode, so it's ready to deploy.
#[derive(Clone, Debug)]
pub struct CustomContractArgs {
    pub contract: CompiledContract,
    pub setup: Vec<FunctionCallDefinition>,
    pub spam: Vec<FunctionCallDefinition>,
}

pub struct ContractMeta {
    name: String,
    filename: String,
    root_dir: String,
}

pub struct ContractArtifacts {
    abi: JsonAbi,
    bytecode: String,
}

#[derive(Debug, Error)]
pub enum ContractMetaError {
    /// ABI not found in file
    #[error("abi not found")]
    ABINotFound,

    #[error("failed to find appropriate ABI for function call: {0}")]
    ABIFunctionNotFound(String),

    #[error("failed to read artifact file at {0}")]
    ArtifactMissing(String),

    #[error("failed to parse JSON from artifact file {0}")]
    ArtifactInvalid(String, serde_json::Error),

    #[error("invalid artifact: bytecode missing")]
    BytecodeMissing,

    #[error("cannot find constructor in ABI")]
    ConstructorNotFound,

    #[error("failed to compile contract: {0}")]
    ContractBuildFailed(String),

    #[error("failed to deserialize ABI: {0}")]
    DeserializeABI(serde_json::Error),

    #[error("failed to run forge build in subprocess: {0}")]
    ForgeBuildFailed(std::io::Error),

    #[error("function with name '{0}' not found in ABI")]
    FunctionNotFound(String),

    #[error("invalid function call: {0}")]
    InvalidCall(String),

    #[error("no solidity files found in the project ({0}): {1}")]
    NoSolidityFiles(String, String),
}

impl ContractMeta {
    pub fn build_source(&self) -> Result<(), ContractMetaError> {
        use ContractMetaError::*;

        let res = Command::new("forge")
            .args([
                "build", //
                "-o",
                ARTIFACTS_PATH, // output artifacts to tmp dir
                "--root",
                &self.root_dir, // project root
            ])
            .output()
            .map_err(ForgeBuildFailed)?;
        debug!("forge build result: {res:?}");
        if !res.status.success() {
            let err_output = String::from_utf8_lossy(&res.stderr).into_owned();
            return Err(ContractBuildFailed(err_output));
        } else {
            let std_out = String::from_utf8_lossy(&res.stdout).into_owned();
            if std_out.to_lowercase().contains("nothing to compile") {
                return Err(NoSolidityFiles(self.root_dir.clone(), std_out));
            }
        }
        Ok(())
    }

    /// Parse artifacts from build output. Must be called after `build_source` has run.
    pub fn parse_artifacts(&self) -> Result<ContractArtifacts, ContractMetaError> {
        use ContractMetaError::*;

        let raw_artifact_path = format!("{ARTIFACTS_PATH}/{}/{}.json", self.filename, self.name);
        let artifact_path = std::path::Path::new(&raw_artifact_path);
        let build_artifact =
            std::fs::read(artifact_path).map_err(|_| ArtifactMissing(raw_artifact_path.clone()))?;
        let artifact_json: serde_json::Value = serde_json::from_slice(&build_artifact)
            .map_err(|e| ArtifactInvalid(raw_artifact_path, e))?;

        // get bytecode
        let bytecode = artifact_json
            .get("bytecode")
            .and_then(|v| v.get("object").and_then(|v| v.as_str()))
            .ok_or(BytecodeMissing)?
            .to_string();

        // get abi
        let abi = artifact_json.get("abi").ok_or(ABINotFound)?;

        // Deserialize ABI into alloy_rs ABI type
        let json_abi: alloy::json_abi::JsonAbi =
            serde_json::from_value(abi.clone()).map_err(DeserializeABI)?;

        Ok(ContractArtifacts {
            abi: json_abi,
            bytecode,
        })
    }
}

impl CustomContractArgs {
    pub fn from_cli_args(args: CustomContractCliArgs) -> Result<Self, CustomContractArgsError> {
        use CustomContractArgsError::*;
        if args.spam_calls.is_empty() {
            return Err(SpamArgsEmpty);
        }

        // read smart contract src
        let contract_meta = args.read_sol_file()?;

        // build solidity source w/ forge
        contract_meta.build_source()?;
        let ContractArtifacts { abi, bytecode } = contract_meta.parse_artifacts()?;

        // get all the function ABIs
        let mut spam_function_calls = vec![];
        let mut setup_function_calls = vec![];
        for spam_call in &args.spam_calls {
            let parsed_fn = NameAndArgs::from_function_call(spam_call)?;
            spam_function_calls.push(parsed_fn);
        }
        for setup_call in &args.setup_calls {
            let parsed_fn = NameAndArgs::from_function_call(setup_call)?;
            setup_function_calls.push(parsed_fn);
        }

        // build contract bytecode, possibly adding constructor args
        let mut contract = CompiledContract::new(bytecode, contract_meta.name);
        if let Ok(constructor_sig) = constructor_sig(&abi) {
            contract = contract.with_constructor_args(constructor_sig, &args.constructor_args())?;
        } else {
            debug!("no constructor found");
        }

        // build spam steps
        let mut spam = vec![];
        for fn_call in spam_function_calls {
            spam.push(
                FunctionCallDefinition::new(contract.template_name())
                    .with_signature(fn_call.signature(&abi)?)
                    .with_args(&fn_call.args),
            );
        }

        // build setup steps
        let mut setup = vec![];
        for fn_call in setup_function_calls {
            setup.push(
                FunctionCallDefinition::new(contract.template_name())
                    .with_signature(fn_call.signature(&abi)?)
                    .with_args(&fn_call.args),
            )
        }

        Ok(CustomContractArgs {
            contract,
            setup,
            spam,
        })
    }
}

fn find_foundry_toml(mut dir: &std::path::Path) -> Option<&std::path::Path> {
    loop {
        if dir.join("foundry.toml").exists() {
            return Some(dir);
        }
        match dir.parent() {
            Some(parent) => dir = parent,
            None => break,
        }
    }
    None
}

pub fn constructor_sig(json_abi: &JsonAbi) -> Result<String, ContractMetaError> {
    let constructor = json_abi
        .constructor()
        .ok_or(ContractMetaError::ConstructorNotFound)?;
    let input_types = constructor
        .inputs
        .iter()
        .map(|input| input.selector_type().into_owned())
        .collect::<Vec<_>>()
        .join(",");
    Ok(format!("({input_types})"))
}

struct NameAndArgs {
    name: String,
    args: Vec<String>,
}

impl NameAndArgs {
    /// Parse the components from a function call.
    ///
    /// Example:
    /// ```rs
    /// let res = parse_function_call("callMe(100, 0xbeef)");
    /// ```
    pub fn from_function_call(fn_call: impl AsRef<str>) -> Result<Self, ContractMetaError> {
        use ContractMetaError::*;

        let call = fn_call.as_ref();
        let open_paren = call
            .find('(')
            .ok_or(InvalidCall(format!("'{call}' missing '('")))?;
        let fn_name = &call[..open_paren];
        if fn_name.is_empty() {
            return Err(InvalidCall(format!("'{call}' function name is empty")));
        }

        let close_paren = call
            .rfind(')')
            .ok_or(InvalidCall(format!("'{call}' missing ')'")))?;
        let args_str = &call[open_paren + 1..close_paren];
        let args: Vec<String> = if args_str.trim().is_empty() {
            vec![]
        } else {
            args_str.split(',').map(|s| s.trim().to_string()).collect()
        };

        Ok(NameAndArgs {
            name: fn_name.to_string(),
            args,
        })
    }

    fn to_fn_call(&self) -> String {
        let Self { name, args } = self;
        format!("{name}({})", args.join(", "))
    }

    fn signature(&self, abi: &JsonAbi) -> Result<String, ContractMetaError> {
        let fn_abis = abi
            .functions
            .get(&self.name)
            .ok_or(ContractMetaError::FunctionNotFound(self.name.to_owned()))?;

        // find the appropriate ABI for the provided args
        let function_abi = fn_abis
            .iter()
            .find(|abi| {
                let sig = abi.signature();
                encode_calldata(&self.args, &sig).is_ok()
            })
            .ok_or(ContractMetaError::ABIFunctionNotFound(self.to_fn_call()))?;

        Ok(function_abi.signature())
    }
}

impl ToTestConfig for CustomContractArgs {
    fn to_testconfig(&self) -> contender_testfile::TestConfig {
        TestConfig::new()
            .with_create(vec![CreateDefinition::new(&self.contract)])
            .with_setup(self.setup.to_owned())
            .with_spam(self.spam.iter().map(SpamRequest::new_tx).collect())
    }
}
