use alloy::primitives::U256;
use contender_core::generator::CompiledContract;

pub const COUNTER: CompiledContract<&'static str> = CompiledContract {
    bytecode: include_str!("./Counter.hex"),
    name: "Counter",
};

pub const SMART_WALLET: CompiledContract<&'static str> = CompiledContract {
    bytecode: include_str!("./SmartWallet.hex"),
    name: "SmartWallet",
};

pub const SPAM_ME: CompiledContract<&'static str> = CompiledContract {
    bytecode: include_str!("./SpamMe.hex"),
    name: "SpamMe5",
};

pub const SPAM_ME_6: CompiledContract<&'static str> = CompiledContract {
    bytecode: include_str!("./SpamMe6.hex"),
    name: "SpamMe6",
};

/// A simple token contract for testing purposes.
/// This contract takes a constructor argument for the initial supply, which must be abi-encoded and appended to the bytecode.
pub const TEST_TOKEN: CompiledContract<&'static str> = CompiledContract {
    bytecode: include_str!("./TestToken.hex"),
    name: "testToken",
};

/// Create a `CompiledContract` for the `TestToken` contract with constructor arguments.
pub fn test_token(token_num: u32, initial_supply: U256) -> CompiledContract<String> {
    let mut bytecode = TEST_TOKEN.bytecode.to_string();
    // Append the initial supply as a 32-byte hex-encoded value
    bytecode.push_str(&format!("{initial_supply:0>64x}"));
    CompiledContract {
        bytecode,
        name: format!("{}{token_num}", TEST_TOKEN.name).to_string(),
    }
}
