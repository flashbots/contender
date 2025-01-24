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
    // Removing inner parenthesis flattens the signature,
    // so we can support signatures that use nested tuples.
    //   e.g. "set((uint256, uint256), uint256)" -> "set(uint256, uint256, uint256)"
    // Alloy doesn't natively support tuples in fn signatures,
    // but we can flatten them to achieve the same effect.
    // Args just have to be passed in flat form.
    let sig = remove_inner_parentheses(sig);
    let func = json_abi::Function::parse(&sig).map_err(|e| {
        ContenderError::SpamError("failed to parse function name", Some(e.to_string()))
    })?;
    let values: Vec<DynSolValue> = args
        .iter()
        .enumerate()
        .map(|(idx, arg)| {
            let mut argtype = String::new();
            func.inputs[idx].full_selector_type_raw(&mut argtype);
            let r#type = DynSolType::parse(&argtype).map_err(|e| {
                ContenderError::SpamError("failed to parse function type", Some(e.to_string()))
            })?;
            r#type.coerce_str(arg.as_ref()).map_err(|e| {
                ContenderError::SpamError(
                    "failed to coerce arg to DynSolValue",
                    Some(e.to_string()),
                )
            })
        })
        .collect::<Result<_>>()?;
    let input = func.abi_encode_input(&values).map_err(|e| {
        ContenderError::SpamError("failed to encode function arguments", Some(e.to_string()))
    })?;
    Ok(input)
}

/// Removes inner parentheses from a string.
pub fn remove_inner_parentheses(input: &str) -> String {
    let mut result = String::new();
    let mut depth = 0;

    for c in input.chars() {
        match c {
            '(' => {
                if depth == 0 {
                    result.push(c); // Keep the outermost opening parenthesis
                }
                depth += 1;
            }
            ')' => {
                depth -= 1;
                if depth == 0 {
                    result.push(c); // Keep the outermost closing parenthesis
                }
            }
            _ => {
                result.push(c); // Always keep the content inside parentheses
            }
        }
    }

    result
}

#[cfg(test)]
pub mod test {
    use super::*;
    use alloy::node_bindings::{Anvil, AnvilInstance};

    pub fn spawn_anvil() -> AnvilInstance {
        Anvil::new().block_time(1).try_spawn().unwrap()
    }

    #[test]
    fn test_remove_inner_parentheses() {
        assert_eq!(remove_inner_parentheses("set(uint256 x)"), "set(uint256 x)");
        assert_eq!(
            remove_inner_parentheses("set((uint256) x)"),
            "set(uint256 x)"
        );
        assert_eq!(
            remove_inner_parentheses("set((uint256, uint256) x)"),
            "set(uint256, uint256 x)"
        );
        assert_eq!(
            remove_inner_parentheses("set((uint256, (uint256, uint256)) x)"),
            "set(uint256, uint256, uint256 x)"
        );
    }
}
