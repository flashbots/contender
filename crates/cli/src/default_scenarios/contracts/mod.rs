use contender_core::generator::types::CompiledContract;

pub const SPAM_ME: CompiledContract<&'static str> = CompiledContract {
    bytecode: include_str!("./SpamMe.hex"),
    name: "SpamMe5",
};
