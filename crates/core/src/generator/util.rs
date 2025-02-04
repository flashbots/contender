use crate::{error::ContenderError, Result};
use alloy::{
    dyn_abi::{DynSolType, DynSolValue, JsonAbiExt},
    json_abi,
};

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
    let func = json_abi::Function::parse(sig)
        .map_err(|e| ContenderError::with_err(e, "failed to parse function signature"))?;
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

#[cfg(test)]
pub mod test {
    use alloy::node_bindings::{Anvil, AnvilInstance};

    pub fn spawn_anvil() -> AnvilInstance {
        Anvil::new().block_time(1).try_spawn().unwrap()
    }
}
