use alloy::{
    primitives::U256,
    providers::RootProvider,
    rpc::types::TransactionRequest,
    transports::http::{Client, Http},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::task::JoinHandle;
use regex::Regex;

pub type RpcProvider = RootProvider<Http<Client>>;

#[derive(Clone, Debug)]
pub struct NamedTxRequest {
    pub name: Option<String>,
    pub tx: TransactionRequest,
}

#[derive(Clone, Deserialize, Debug, Serialize)]
pub struct FunctionCallDefinition {
    /// Address of the contract to call.
    pub to: String,
    /// Address of the tx sender.
    pub from: String,
    /// Name of the function to call.
    pub signature: String,
    /// Parameters to pass to the function.
    pub args: Option<Vec<String>>,
    /// Value in wei to send with the tx.
    pub value: Option<String>,
    /// Parameters to fuzz during the test.
    pub fuzz: Option<Vec<FuzzParam>>,
    /// Optional type of the spam transaction for categorization.
    pub kind: Option<String>
}

#[derive(Clone, Deserialize, Debug, Serialize)]
pub struct CreateDefinition {
    /// Bytecode of the contract to deploy.
    pub bytecode: String,
    /// Name to identify the contract later.
    pub name: String,
    /// Address of the tx sender.
    pub from: String,
}

#[derive(Clone, Deserialize, Debug, Serialize)]
pub struct FuzzParam {
    /// Name of the parameter to fuzz.
    pub param: String,
    /// Minimum value fuzzer will use.
    pub min: Option<U256>,
    /// Maximum value fuzzer will use.
    pub max: Option<U256>,
}

#[derive(Debug)]
pub struct Plan {
    pub env: HashMap<String, String>,
    pub create_steps: Vec<NamedTxRequest>,
    pub setup_steps: Vec<NamedTxRequest>,
    pub spam_steps: Vec<NamedTxRequest>,
}

pub type CallbackResult = crate::Result<Option<JoinHandle<()>>>;

pub enum PlanType<F: Fn(NamedTxRequest) -> CallbackResult> {
    Create(F),
    Setup(F),
    Spam(usize, F),
}

impl FunctionCallDefinition {
    /// Constructs a function name string combining signature and arguments.
    pub fn name(&self) -> String {
        // Use regex to parse the function signature
        let signature = &self.signature;
        let re = Regex::new(r"(?P<func_name>\w+)\s*\((?P<params>[^\)]*)\)(\s*returns\s*\((?P<returns>[^\)]*)\))?").unwrap();

        // If the signature matches the expected pattern
        if let Some(caps) = re.captures(signature) {
            let func_name = caps.name("func_name").unwrap().as_str();
            let params_str = caps.name("params").unwrap().as_str();

            // Split the parameters into a vector
            let params: Vec<&str> = params_str.split(',').map(|s| s.trim()).collect();

            // Prepare a vector to hold the reconstructed parameters
            let mut reconstructed_params = Vec::new();

            let values = self.args.as_deref().unwrap_or(&[]);

            // Iterate over parameters and associate them with args[i]
            for (i, param) in params.iter().enumerate() {
                // Split each parameter into type and name
                let parts: Vec<&str> = param.split_whitespace().collect();
                let param_value = values.get(i).map(String::as_str).unwrap_or("");
                if parts.len() == 2 {
                    let param_type = parts[0];
                    let param_name = parts[1];
                    reconstructed_params.push(format!("{} {}=[{}]", param_type, param_name, param_value));
                }
            }

            // Reconstruct the function signature
            let reconstructed_signature = format!("{}({})", func_name, reconstructed_params.join(", "));

            return reconstructed_signature;
        }

        // If the signature doesn't match, return it as is
        signature.clone()
    }
}

#[cfg(test)]
pub mod tests {
    use super::FunctionCallDefinition;

    #[test]
    fn constructs_name_from_signature() {
        let fn_def_with_args = FunctionCallDefinition {
            to: "0x1234".to_string(),
            from: "0x5678".to_string(),
            signature: "transfer(address to, uint256 amount)".to_string(),
            args: Some(vec!["0x8765".to_string(), "100".to_string()]),
            value: Some("100".to_string()),
            fuzz: None,
            kind: None,
        };
        assert_eq!(fn_def_with_args.name(), "transfer(address to=[0x8765], uint256 amount=[100])");

        let fn_def_no_args = FunctionCallDefinition {
            to: "0x1234".to_string(),
            from: "0x5678".to_string(),
            signature: "transfer()".to_string(),
            args: None,
            value: Some("100".to_string()),
            fuzz: None,
            kind: None,
        };
        assert_eq!(fn_def_no_args.name(), "transfer()");

        // checks  if it does not fail against invalid signature.
        let fn_def_invalid_sig = FunctionCallDefinition {
            to: "0x1234".to_string(),
            from: "0x5678".to_string(),
            signature: "transfer(address, uint256 amount)".to_string(),
            args: Some(vec!["0x8765".to_string(), "100".to_string()]),
            value: Some("100".to_string()),
            fuzz: None,
            kind: None,
        };
        assert_eq!(fn_def_invalid_sig.name(), "transfer(uint256 amount=[100])");
    }
}
