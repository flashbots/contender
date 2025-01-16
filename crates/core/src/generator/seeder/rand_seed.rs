use super::{SeedValue, Seeder};
use alloy::primitives::{keccak256, U256};
use rand::Rng;

/// Default seed generator, using a random 32-byte seed.
#[derive(Debug, Clone)]
pub struct RandSeed {
    seed: [u8; 32],
}

/// Copies `seed` into `target` and right-pads with `0x01` to 32 bytes.
fn fill_bytes(seed: &[u8], target: &mut [u8; 32]) {
    if seed.len() < 32 {
        target[0..seed.len()].copy_from_slice(seed);
        target[seed.len()..32].fill(0x01);
    } else {
        target.copy_from_slice(&seed[0..32]);
    }
}

impl RandSeed {
    pub fn new() -> Self {
        let mut rng = rand::thread_rng();
        let mut seed = [0u8; 32];
        rng.fill(&mut seed);
        Self { seed }
    }

    /// Interprets `seed` as a byte array.
    /// - If `seed` is less than 32 bytes, it is right-padded with 0x01.
    /// - If `seed` is more than 32 bytes, only the first 32 bytes are used.
    /// - Number types created from these bytes are interpreted as big-endian.
    pub fn seed_from_bytes(seed_bytes: &[u8]) -> Self {
        let mut seed_arr = [0u8; 32];
        fill_bytes(seed_bytes, &mut seed_arr);
        Self { seed: seed_arr }
    }

    /// Interprets seed as a number in base 10 or 16.
    pub fn seed_from_str(seed: &str) -> Self {
        let (radix, seed) = if seed.starts_with("0x") {
            (16u64, seed.split_at(2).1)
        } else {
            (10u64, seed)
        };
        let n =
            U256::from_str_radix(seed, radix).expect("invalid seed number; must fit in 32 bytes");
        Self::seed_from_u256(n)
    }

    pub fn seed_from_u256(seed: U256) -> Self {
        Self {
            seed: seed.to_be_bytes(),
        }
    }
}

impl SeedValue for RandSeed {
    fn as_bytes(&self) -> &[u8] {
        &self.seed
    }

    fn as_u64(&self) -> u64 {
        let mut seed: [u8; 8] = [0; 8];
        seed.copy_from_slice(&self.seed[24..32]);
        u64::from_be_bytes(seed)
    }

    fn as_u128(&self) -> u128 {
        let mut seed: [u8; 16] = [0; 16];
        seed.copy_from_slice(&self.seed[16..32]);
        u128::from_be_bytes(seed)
    }

    fn as_u256(&self) -> U256 {
        U256::from_be_bytes::<32>(self.seed)
    }
}

impl Seeder for RandSeed {
    fn seed_values(
        &self,
        amount: usize,
        min: Option<U256>,
        max: Option<U256>,
    ) -> Box<impl Iterator<Item = impl SeedValue>> {
        let min = min.unwrap_or(U256::ZERO);
        let max = max.unwrap_or(U256::MAX);
        assert!(min < max, "min must be less than max");
        let vals = (0..amount).map(move |i| {
            // generate random-looking value between min and max from seed
            let seed_num = self.as_u256() + U256::from(i);
            let val = keccak256(seed_num.as_le_slice());
            let val = U256::from_be_bytes(val.0);
            let val = val % (max - min) + min;
            RandSeed::seed_from_u256(val)
        });
        Box::new(vals)
    }
}

impl Default for RandSeed {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use alloy::hex::ToHexExt;

    use super::U256;
    use crate::generator::seeder::SeedValue;

    #[test]
    fn encodes_seed_bytes() {
        let mut seed_bytes = [0u8; 32];
        seed_bytes[seed_bytes.len() - 1] = 0x01;
        println!("{}", seed_bytes.encode_hex());
        let seed = super::RandSeed::seed_from_bytes(&seed_bytes);
        println!("{}", seed.as_bytes().encode_hex());
        assert_eq!(seed.as_bytes().len(), 32);
        assert_eq!(seed.as_u64(), 1);
        assert_eq!(seed.as_u128(), 1);
        assert_eq!(seed.as_u256(), U256::from(1));
    }

    #[test]
    fn encodes_seed_string() {
        let seed = super::RandSeed::seed_from_str("0x01");
        assert_eq!(seed.as_u64(), 1);
        assert_eq!(seed.as_u128(), 1);
        assert_eq!(seed.as_u256(), U256::from(1));
    }

    #[test]
    fn encodes_seed_u256() {
        let n = U256::MAX;
        let seed = super::RandSeed::seed_from_u256(n);
        assert_eq!(seed.as_u256(), n);
    }
}
