pub const VALUE_KEY: &str = "__tx_value_contender__";
pub const SENDER_KEY: &str = "_sender";
pub const SETCODE_KEY: &str = "_setCodeSender";

fn placeholder(name: &str) -> String {
    format!("{{{name}}}")
}

pub fn sender_placeholder() -> String {
    placeholder(SENDER_KEY)
}

pub fn setcode_placeholder() -> String {
    placeholder(SETCODE_KEY)
}
