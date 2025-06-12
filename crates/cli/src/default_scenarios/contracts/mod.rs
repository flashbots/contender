use alloy::primitives::U256;
use contender_core::generator::types::CompiledContract;

pub const SPAM_ME: CompiledContract<&'static str> = CompiledContract {
    bytecode: include_str!("./SpamMe.hex"),
    name: "SpamMe5",
};

/// A simple token contract for testing purposes.
/// This contract takes a constructor argument for the initial supply, which must be abi-encoded and appended to the bytecode.
pub const TEST_TOKEN: CompiledContract<&'static str> = CompiledContract {
    bytecode: include_str!("./TestToken.hex"),
    name: "testToken",
};

/// Helper function to create a `CompiledContract` for the `SpamMe` contract with constructor arguments.
pub fn test_token(token_num: u32, initial_supply: U256) -> CompiledContract<String> {
    let mut bytecode = TEST_TOKEN.bytecode.to_string();
    // Append the initial supply as a 32-byte hex-encoded value
    bytecode.push_str(&format!("{:0>64x}", initial_supply));
    CompiledContract {
        bytecode,
        name: format!("{}{token_num}", TEST_TOKEN.name).to_string(),
    }
}
