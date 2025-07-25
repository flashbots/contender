use crate::{error::ContenderError, Result};
use alloy::{
    consensus::TxType,
    dyn_abi::{DynSolType, DynSolValue, JsonAbiExt},
    json_abi,
    rpc::types::TransactionRequest,
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
        .map_err(|e| ContenderError::with_err(e, "failed to parse function signature"))?;
    if func.inputs.len() != args.len() {
        return Err(ContenderError::GenericError(
            "invalid args for function signature:",
            format!(
                "{sig}: {} param(s) in sig, {} args provided",
                func.inputs.len(),
                args.len(),
            ),
        ));
    }
    let values: Vec<DynSolValue> = args
        .iter()
        .enumerate()
        .map(|(idx, arg)| {
            let mut argtype = String::new();
            func.inputs[idx].full_selector_type_raw(&mut argtype);
            let r#type = DynSolType::parse(&argtype)
                .map_err(|e| ContenderError::with_err(e, "failed to parse function type"))?;
            r#type.coerce_str(arg.as_ref()).map_err(|e| {
                ContenderError::SpamError(
                    "failed to coerce arg to DynSolValue",
                    Some(e.to_string()),
                )
            })
        })
        .collect::<Result<_>>()?;
    let input = func
        .abi_encode_input(&values)
        .map_err(|e| ContenderError::with_err(e, "failed to encode function arguments"))?;
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

#[cfg(test)]
pub mod test {
    use alloy::node_bindings::{Anvil, AnvilInstance};

    pub fn spawn_anvil() -> AnvilInstance {
        Anvil::new().block_time(1).try_spawn().unwrap()
    }
}
