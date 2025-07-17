use alloy::{
    consensus::BlobTransactionSidecar,
    primitives::{Address, U256},
};
use serde::{Deserialize, Serialize};

/// User-facing definition of a function call to be executed.
#[derive(Clone, Deserialize, Debug, Serialize)]
pub struct FunctionCallDefinition {
    /// Address of the contract to call.
    pub to: String,
    /// Address of the tx sender.
    pub from: Option<String>,
    /// Get a `from` address from the pool of signers specified here.
    pub from_pool: Option<String>,
    /// Name of the function to call.
    pub signature: Option<String>,
    /// Parameters to pass to the function.
    pub args: Option<Vec<String>>,
    /// Value in wei to send with the tx.
    pub value: Option<String>,
    /// Parameters to fuzz during the test.
    pub fuzz: Option<Vec<FuzzParam>>,
    /// Optional type of the spam transaction for categorization.
    pub kind: Option<String>,
    /// Optional gas limit, which will skip gas estimation. This allows reverting txs to be sent.
    pub gas_limit: Option<u64>,
    /// Optional blob data; tx type must be set to EIP4844 by spammer
    pub blob_data: Option<String>,
}

/// User-facing definition of a function call to be executed.
#[derive(Clone, Deserialize, Debug, Serialize)]
pub struct BundleCallDefinition {
    #[serde(rename = "tx")]
    pub txs: Vec<FunctionCallDefinition>,
}

impl FunctionCallDefinition {
    pub fn new(to: impl AsRef<str>) -> Self {
        FunctionCallDefinition {
            to: to.as_ref().to_owned(),
            from: None,
            from_pool: None,
            signature: None,
            args: None,
            value: None,
            fuzz: None,
            kind: None,
            gas_limit: None,
            blob_data: None,
        }
    }

    pub fn with_signature(mut self, sig: impl AsRef<str>) -> Self {
        self.signature = Some(sig.as_ref().to_owned());
        self
    }
    pub fn with_from(mut self, from: impl AsRef<str>) -> Self {
        self.from = Some(from.as_ref().to_owned());
        self
    }
    pub fn with_from_pool(mut self, from_pool: impl AsRef<str>) -> Self {
        self.from_pool = Some(from_pool.as_ref().to_owned());
        self
    }
    pub fn with_args(mut self, args: &[impl AsRef<str>]) -> Self {
        self.args = Some(
            args.iter()
                .map(|t| t.as_ref().to_owned())
                .collect::<Vec<_>>(),
        );
        self
    }
    /// Set value in wei to send with the tx.
    pub fn with_value(mut self, value: U256) -> Self {
        self.value = Some(value.to_string());
        self
    }
    pub fn with_fuzz(mut self, fuzz: &[FuzzParam]) -> Self {
        self.fuzz = Some(fuzz.to_vec());
        self
    }
    pub fn with_kind(mut self, kind: impl AsRef<str>) -> Self {
        self.kind = Some(kind.as_ref().to_owned());
        self
    }
    pub fn with_gas_limit(mut self, gas_limit: u64) -> Self {
        self.gas_limit = Some(gas_limit);
        self
    }
    pub fn with_blob_data(mut self, blob_data: impl AsRef<str>) -> Self {
        self.blob_data = Some(blob_data.as_ref().to_owned());
        self
    }
}

pub struct FunctionCallDefinitionStrict {
    pub to: String, // may be a placeholder, so we can't use Address
    pub from: Address,
    pub signature: String,
    pub args: Vec<String>,
    pub value: Option<String>,
    pub fuzz: Vec<FuzzParam>,
    pub kind: Option<String>,
    pub gas_limit: Option<u64>,
    pub sidecar: Option<BlobTransactionSidecar>,
}

#[derive(Clone, Deserialize, Debug, Serialize)]
pub struct FuzzParam {
    /// Name of the parameter to fuzz.
    pub param: Option<String>,
    /// Fuzz the `value` field of the tx (ETH sent with the tx).
    pub value: Option<bool>,
    /// Minimum value fuzzer will use.
    pub min: Option<U256>,
    /// Maximum value fuzzer will use.
    pub max: Option<U256>,
}
