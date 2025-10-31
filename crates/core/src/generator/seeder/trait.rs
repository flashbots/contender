use alloy::primitives::U256;

pub trait Seeder {
    fn seed_values(
        &self,
        amount: usize,
        min: Option<U256>,
        max: Option<U256>,
    ) -> Box<impl Iterator<Item = impl SeedValue>>;

    fn seed_from_u256(seed: U256) -> Self;
    fn seed_from_bytes(seed: &[u8]) -> Self;
    fn seed_from_str(seed: &str) -> Self;
}

pub trait SeedValue {
    fn as_bytes(&self) -> &[u8];
    fn as_u64(&self) -> u64;
    fn as_u128(&self) -> u128;
    fn as_u256(&self) -> U256;
}
