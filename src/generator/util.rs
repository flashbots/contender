use alloy::{
    providers::RootProvider,
    transports::http::{Client, Http},
};

pub type RpcProvider = RootProvider<Http<Client>>;

#[cfg(test)]
pub mod test {
    use alloy::node_bindings::{Anvil, AnvilInstance};

    pub fn spawn_anvil() -> AnvilInstance {
        Anvil::new().block_time(1).try_spawn().unwrap()
    }
}
