use crate::default_scenarios::builtin::ToTestConfig;
use crate::util::bold;
use alloy::json_abi::JsonAbi;
use contender_core::error::ContenderError;
use contender_core::generator::types::SpamRequest;
use contender_core::generator::util::encode_calldata;
use contender_core::generator::{CompiledContract, CreateDefinition, FunctionCallDefinition};
use contender_core::Result;
use contender_testfile::TestConfig;
use std::process::Command;
use tracing::debug;

const ARTIFACTS_PATH: &str = "/tmp/contender-contracts";

#[derive(Clone, Debug, clap::Parser)]
pub struct CustomContractCliArgs {
    /// Path to smart contract source. Format: <path/to/contract.sol>:<ContractName>
    contract_path: std::path::PathBuf,

    #[arg(
        long = "constructor-sig",
        visible_aliases = &["cs"],
        long_help = "Constructor function signature. Format: \"(uint256 x, address a)\""
    )]
    constructor_sig: Option<String>,

    // TODO: revise constructor flags; use the same format as spam_calls
    #[arg(
        long = "constructor-arg",
        visible_aliases = &["ca"],
        action = clap::ArgAction::Append,
        long_help = "Constructor arguments. May be specified multiple times.",
    )]
    constructor_args: Vec<String>,

    #[arg(
        long = "spam",
        short,
        num_args = 1,
        action = clap::ArgAction::Append,
        help = "Spam function calls. May be specified multiple times. Format: \"functionName(...args)\"",
        long_help = "Spam function calls. May be specified multiple times. Example: `--spam \"setNumber(123456)\"`"
    )]
    spam_calls: Vec<String>,
}

/// This contract is expected to have its constructor args already appended to the bytecode, so it's ready to deploy.
#[derive(Clone, Debug)]
pub struct CustomContractArgs {
    pub contract: CompiledContract,
    pub spam: Vec<FunctionCallDefinition>,
}

impl CustomContractArgs {
    pub fn from_cli_args(args: CustomContractCliArgs) -> Result<Self> {
        if args.spam_calls.is_empty() {
            return Err(ContenderError::SpamError(
                "invalid CLI params:",
                Some(format!(
                    "must provide at least one {} argument",
                    bold("--spam")
                )),
            ));
        }

        // read smart contract src
        // format should be <path/to/contract.sol>:<ContractName>
        let contract_path = args
            .contract_path
            .to_str()
            .ok_or(ContenderError::GenericError(
                "invalid contract path",
                String::new(),
            ))?;

        let re = regex::Regex::new(r"^(.+\.sol):([A-Za-z_][A-Za-z0-9_]*)$")
            .map_err(|e| ContenderError::GenericError("failed to compile regex", e.to_string()))?;
        let caps = re.captures(contract_path).ok_or_else(|| {
            ContenderError::GenericError(
                "invalid contract path format; expected <path/to/contract.sol>:<ContractName>",
                contract_path.to_owned(),
            )
        })?;
        let contract_path = caps
            .get(1)
            .ok_or(ContenderError::GenericError(
                "invalid contract path",
                contract_path.to_owned(),
            ))?
            .as_str();
        let contract_name = caps
            .get(2)
            .ok_or(ContenderError::GenericError(
                "invalid contract name",
                contract_path.to_owned(),
            ))?
            .as_str();

        let contract_filename = std::path::Path::new(contract_path)
            .file_name()
            .and_then(|f| f.to_str())
            .ok_or_else(|| {
                ContenderError::GenericError(
                    "failed to extract contract filename",
                    contract_path.to_owned(),
                )
            })?;

        let contract_path_obj = std::path::Path::new(contract_path);
        let parent_dir = contract_path_obj.parent().ok_or_else(|| {
            ContenderError::GenericError(
                "failed to get contract parent directory",
                contract_path.to_owned(),
            )
        })?;

        // try to find root directory containing foundry.toml, otherwise assume the source file's parent dir
        let mut root_dir = parent_dir;
        if let Some(foundry_dir) = find_foundry_toml(parent_dir) {
            root_dir = foundry_dir;
        }
        let root_dir = root_dir.to_str().ok_or(ContenderError::SpamError(
            "failed to convert project root directory to str",
            Some(format!("{root_dir:?}")),
        ))?;

        // compile with forge; runs `forge build` in the project root
        let res = Command::new("forge")
            .args([
                "build", //
                "-o",
                ARTIFACTS_PATH, // output artifacts to tmp dir
                "--root",
                root_dir, // project root
            ])
            .output()
            .map_err(|e| ContenderError::with_err(e, "failed to run forge in subprocess"))?;
        debug!("forge build result: {res:?}");
        if !res.status.success() {
            let err_output = String::from_utf8_lossy(&res.stderr).into_owned();
            return Err(ContenderError::SpamError(
                "failed to compile contract",
                Some(err_output),
            ));
        } else {
            let std_out = String::from_utf8_lossy(&res.stdout).into_owned();
            if std_out.to_lowercase().contains("nothing to compile") {
                return Err(ContenderError::SpamError(
                    "no solidity files found in the given directory:",
                    Some(std_out),
                ));
            }
        }

        // read artifact file, decode json
        let artifact_path = std::path::Path::new(&format!(
            "{ARTIFACTS_PATH}/{contract_filename}/{contract_name}.json"
        ))
        .to_owned();
        let build_artifact = std::fs::read(artifact_path)
            .map_err(|e| ContenderError::with_err(e, "failed to read artifact file"))?;
        let artifact_json: serde_json::Value = serde_json::from_slice(&build_artifact)
            .map_err(|e| ContenderError::with_err(e, "failed to parse artifact JSON"))?;

        // get bytecode
        let bytecode = artifact_json
            .get("bytecode")
            .and_then(|v| v.get("object").and_then(|v| v.as_str()))
            .ok_or_else(|| {
                ContenderError::GenericError("missing bytecode in artifact", String::new())
            })?
            .to_string();

        // get abi
        let abi = artifact_json
            .get("abi")
            .ok_or(ContenderError::GenericError("ABI not found", String::new()))?;

        // Deserialize ABI into alloy_rs ABI type
        let json_abi: alloy::json_abi::JsonAbi =
            serde_json::from_value(abi.clone()).map_err(|e| {
                ContenderError::with_err(e, "failed to deserialize ABI into alloy_rs type")
            })?;

        // get all the function ABIs
        let mut function_calls = vec![];
        for spam_call in &args.spam_calls {
            let parsed_fn = NameAndArgs::from_function_call(spam_call)?;
            function_calls.push(parsed_fn);
        }

        let mut contract = CompiledContract::new(bytecode, contract_name.to_owned());
        if let Some(constructor_sig) = args.constructor_sig {
            contract = contract.with_constructor_args(constructor_sig, &args.constructor_args)?;
        }

        let mut spam = vec![];
        for fn_call in function_calls {
            spam.push(
                FunctionCallDefinition::new(contract.template_name())
                    .with_from_pool("spammers")
                    .with_signature(fn_call.signature(&json_abi)?)
                    .with_args(&fn_call.args),
            );
        }

        return Ok(CustomContractArgs { contract, spam });
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
    pub fn from_function_call(fn_call: impl AsRef<str>) -> Result<Self> {
        let call = fn_call.as_ref();
        let open_paren = call.find('(').ok_or_else(|| {
            ContenderError::GenericError(
                "invalid function call format; missing '('",
                call.to_string(),
            )
        })?;
        let fn_name = &call[..open_paren];
        if fn_name.is_empty() {
            return Err(ContenderError::GenericError(
                "function name is empty",
                call.to_string(),
            ));
        }

        let close_paren = call.rfind(')').ok_or_else(|| {
            ContenderError::GenericError(
                "invalid function call format; missing ')'",
                call.to_string(),
            )
        })?;
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

    fn signature(&self, abi: &JsonAbi) -> Result<String> {
        let fn_abis = abi
            .functions
            .get(&self.name)
            .ok_or(ContenderError::GenericError(
                "function name was not found in contract's ABI:",
                self.name.to_owned(),
            ))?;

        // find the appropriate ABI for the provided args
        let function_abi = fn_abis
            .iter()
            .find_map(|abi| {
                let sig = abi.signature();
                if encode_calldata(&self.args, &sig).is_ok() {
                    Some(abi)
                } else {
                    None
                }
            })
            .ok_or(ContenderError::GenericError(
                "failed to find appropriate ABI for function call:",
                format!("({})", self.to_fn_call()),
            ))?;

        Ok(function_abi.signature())
    }
}

impl ToTestConfig for CustomContractArgs {
    fn to_testconfig(&self) -> contender_testfile::TestConfig {
        TestConfig::new()
            .with_create(vec![
                CreateDefinition::new(&self.contract).with_from_pool("admin")
            ])
            .with_spam(self.spam.iter().map(SpamRequest::new_tx).collect())
    }
}
