use crate::{
    error::Error,
    generator::seeder::{SeedValue, Seeder},
    Result,
};
use alloy::{
    consensus::TxType,
    dyn_abi::{DynSolType, DynSolValue, JsonAbiExt},
    json_abi,
    primitives::FixedBytes,
    rpc::types::TransactionRequest,
    signers::local::PrivateKeySigner,
};
use tracing::info;

/// Encode the calldata for a function signature given an array of string arguments.
///
/// ## Example
/// ```
/// use contender_core::generator::util::encode_calldata;
/// use alloy::hex::ToHexExt;
///
/// let args = vec!["0x12345678"];
/// let sig = "set(uint256 x)";
/// let calldata = encode_calldata(&args, sig).unwrap();
/// assert_eq!(calldata.encode_hex(), "60fe47b10000000000000000000000000000000000000000000000000000000012345678");
/// ```
pub fn encode_calldata(args: &[impl AsRef<str>], sig: &str) -> Result<Vec<u8>> {
    if sig.is_empty() {
        return Ok(vec![]);
    }
    let func = json_abi::Function::parse(sig)
        .map_err(|e| Error::Config(format!("failed to parse function signature: {e}")))?;
    if func.inputs.len() != args.len() {
        return Err(Error::Config(format!(
            "invalid args for function signature '{sig}': {} param(s) in sig, {} args provided",
            func.inputs.len(),
            args.len(),
        )));
    }
    let values: Vec<DynSolValue> = args
        .iter()
        .enumerate()
        .map(|(idx, arg)| {
            let mut argtype = String::new();
            func.inputs[idx].full_selector_type_raw(&mut argtype);
            let r#type = DynSolType::parse(&argtype).map_err(Error::DynAbi)?;
            r#type.coerce_str(arg.as_ref()).map_err(Error::DynAbi)
        })
        .collect::<Result<_>>()?;
    let input = func.abi_encode_input(&values).map_err(Error::DynAbi)?;
    Ok(input)
}

/// Sets eip-specific fields on a `&mut TransactionRequest`.
/// `chain_id` is ignored for Legacy transactions.
pub fn complete_tx_request(
    tx_req: &mut TransactionRequest,
    tx_type: TxType,
    gas_price: u128,
    priority_fee: u128,
    gas_limit: u64,
    chain_id: u64,
    blob_gas_price: u128,
) {
    match tx_type {
        TxType::Legacy => {
            tx_req.gas_price = Some(gas_price + 4_200_000_000);
        }
        TxType::Eip1559 => {
            tx_req.max_fee_per_gas = Some(gas_price + (gas_price / 5));
            tx_req.max_priority_fee_per_gas = Some(priority_fee);
            tx_req.chain_id = Some(chain_id);
        }
        TxType::Eip4844 => {
            tx_req.max_fee_per_blob_gas = Some(blob_gas_price + (blob_gas_price / 5));
            // recurse with eip1559 to get gas params
            complete_tx_request(
                tx_req,
                TxType::Eip1559,
                gas_price,
                priority_fee,
                gas_limit,
                chain_id,
                blob_gas_price,
            );
        }
        TxType::Eip7702 => {
            // recurse with eip1559 to get gas params
            complete_tx_request(
                tx_req,
                TxType::Eip1559,
                gas_price,
                priority_fee,
                gas_limit,
                chain_id,
                blob_gas_price,
            );
        }
        _ => {
            info!("Unsupported tx type: {tx_type:?}, defaulting to legacy");
            // recurse with legacy type
            complete_tx_request(
                tx_req,
                TxType::Legacy,
                gas_price,
                priority_fee,
                gas_limit,
                chain_id,
                blob_gas_price,
            );
        }
    };
    tx_req.gas = Some(gas_limit);
}

pub fn generate_setcode_signer(seed: &impl Seeder) -> (PrivateKeySigner, [u8; 32]) {
    let raw_seed = seed
        .seed_values(9001, None, None)
        .last()
        .expect("failed to generate seed values for setcode signer");
    let seed_bytes = raw_seed.as_bytes();
    (
        PrivateKeySigner::from_slice(seed_bytes)
            .expect("failed to parse seed value into private key"),
        FixedBytes::from_slice(seed_bytes).0,
    )
}

#[cfg(test)]
pub mod test {
    use alloy::node_bindings::{Anvil, AnvilInstance};

    pub fn spawn_anvil() -> AnvilInstance {
        Anvil::new().block_time_f64(0.25).try_spawn().unwrap()
    }
}
