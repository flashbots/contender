pub mod rand_seed;
use alloy::primitives::U256;

pub trait Seeder {
    fn seed_values(
        &self,
        amount: usize,
        min: Option<U256>,
        max: Option<U256>,
    ) -> Box<Vec<impl SeedValue>>;
}

pub trait SeedValue {
    fn as_bytes(&self) -> &[u8];
    fn as_u64(&self) -> u64;
    fn as_u128(&self) -> u128;
    fn as_u256(&self) -> U256;
}
