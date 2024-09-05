use alloy::primitives::U256;
use rand::Rng;

#[derive(Debug, Clone)]
pub struct RandSeed {
    pub seed: [u8; 32],
}

impl RandSeed {
    pub fn new() -> Self {
        let mut rng = rand::thread_rng();
        let mut seed = [0u8; 32];
        rng.fill(&mut seed);
        Self { seed }
    }

    pub fn from_bytes(seed: &[u8]) -> Self {
        let mut seed_arr = [0u8; 32];
        seed_arr.copy_from_slice(seed);
        Self { seed: seed_arr }
    }

    pub fn from_str(seed: &str) -> Self {
        let mut seed_arr = [0u8; 32];
        seed_arr.copy_from_slice(seed.as_bytes());
        Self { seed: seed_arr }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.seed
    }

    pub fn as_u64(&self) -> u64 {
        u64::from_le_bytes(self.seed[0..8].try_into().unwrap())
    }

    pub fn as_u128(&self) -> u128 {
        u128::from_le_bytes(self.seed[0..16].try_into().unwrap())
    }

    pub fn as_u256(&self) -> U256 {
        U256::from_le_bytes::<32>(self.seed.try_into().unwrap())
    }
}

impl Default for RandSeed {
    fn default() -> Self {
        Self::new()
    }
}
