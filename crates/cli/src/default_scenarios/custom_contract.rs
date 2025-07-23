use crate::default_scenarios::builtin::ToTestConfig;
use contender_core::error::ContenderError;
use contender_core::generator::types::SpamRequest;
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

    #[arg(
        long = "constructor-arg",
        visible_aliases = &["ca"],
        action = clap::ArgAction::Append,
        long_help = "Constructor arguments. May be specified multiple times.",
    )]
    constructor_args: Vec<String>,

    spam_sig: String,

    #[arg(
        long = "spam-arg",
        visible_aliases = &["sa"],
        action = clap::ArgAction::Append,
        long_help = "Arguments to the function called by the spammer. May be specified multiple times."
    )]
    spam_args: Vec<String>,
}

/// This contract is expected to have its constructor args already appended to the bytecode, so it's ready to deploy.
#[derive(Clone, Debug)]
pub struct CustomContractArgs {
    pub contract: CompiledContract,
    pub spam: Vec<FunctionCallDefinition>,
    // TODO: change the spam function cli syntax s.t. we can write `--spam "myFunction(123, 456)" --spam "otherFn(0xbeef)"`
    // we'll have to read the ABI artifact to get the sig ourselves, rather than relying on the user to provide it
    // but then we
    // (1) simplify the UX a lot,
    // (2) users can provide multiple spam calls, and
    // (3) we could use this to add setup steps as well
}

impl CustomContractArgs {
    pub fn from_cli_args(args: CustomContractCliArgs) -> Result<Self> {
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

        // read artifact file, decode json to get bytecode
        let artifact_path = std::path::Path::new(&format!(
            "{ARTIFACTS_PATH}/{contract_filename}/{contract_name}.json"
        ))
        .to_owned();
        let build_artifact = std::fs::read(artifact_path)
            .map_err(|e| ContenderError::with_err(e, "failed to read artifact file"))?;
        let artifact_json: serde_json::Value = serde_json::from_slice(&build_artifact)
            .map_err(|e| ContenderError::with_err(e, "failed to parse artifact JSON"))?;
        let bytecode = artifact_json
            .get("bytecode")
            .and_then(|v| v.get("object").and_then(|v| v.as_str()))
            .ok_or_else(|| {
                ContenderError::GenericError("missing bytecode in artifact", String::new())
            })?
            .to_string();

        let mut contract = CompiledContract::new(bytecode, contract_name.to_owned());
        if let Some(constructor_sig) = args.constructor_sig {
            contract = contract.with_constructor_args(constructor_sig, &args.constructor_args)?;
        }

        let spam = vec![FunctionCallDefinition::new(contract.template_name())
            .with_from_pool("spammers")
            .with_signature(args.spam_sig)
            .with_args(&args.spam_args)];

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

impl ToTestConfig for CustomContractArgs {
    fn to_testconfig(&self) -> contender_testfile::TestConfig {
        TestConfig::new()
            .with_create(vec![
                CreateDefinition::new(&self.contract).with_from_pool("admin")
            ])
            .with_spam(self.spam.iter().map(SpamRequest::new_tx).collect())
    }
}
