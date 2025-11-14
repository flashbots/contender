use alloy::primitives::U256;
use ark_bn254::{Fq, Fq2, G1Affine, G1Projective, G2Affine, G2Projective, Fr};
use ark_ff::BigInteger256;
use std::sync::LazyLock;

// Precompute the G1 generator in projective coordinates for efficiency
static G1_GENERATOR_PROJ: LazyLock<G1Projective> = LazyLock::new(|| {
    // BN254 G1 generator is (1, 2) in the base field
    let x = Fq::new(hex_to_bigint256("0000000000000000000000000000000000000000000000000000000000000001"));
    let y = Fq::new(hex_to_bigint256("0000000000000000000000000000000000000000000000000000000000000002"));
    let generator = G1Affine::new(x, y);
    G1Projective::from(generator)
});

// Precompute the generator in projective coordinates for efficiency
static G2_GENERATOR_PROJ: LazyLock<G2Projective> = LazyLock::new(|| {
    let x0 = Fq::new(hex_to_bigint256("1800DEEF121F1E76426A00665E5C4479674322D4F75EDADD46DEBD5CD992F6ED"));
    let x1 = Fq::new(hex_to_bigint256("198E9393920D483A7260BFB731FB5D25F1AA493335A9E71297E485B7AEF312C2"));
    let y0 = Fq::new(hex_to_bigint256("12C85EA5DB8C6DEB4AAB71808DCB408FE3D1E7690C43D37B4CE6CC0166FA7DAA"));
    let y1 = Fq::new(hex_to_bigint256("090689D0585FF075EC9E99AD690C3395BC4B313370B38EF355ACDADCD122975B"));
    
    let generator = G2Affine::new(Fq2::new(x0, x1), Fq2::new(y0, y1));
    G2Projective::from(generator)
});

// Helper function to convert hex string (without 0x prefix) to BigInteger256
fn hex_to_bigint256(hex: &str) -> BigInteger256 {
    use alloy::hex::FromHex;
    let bytes: [u8; 32] = <[u8; 32]>::from_hex(hex).expect("Invalid hex string");
    
    // Construct BigInteger256 from big-endian bytes
    // BigInteger256 is BigInt<4>, which is 4 u64 limbs in little-endian order
    let mut limbs = [0u64; 4];
    for (i, chunk) in bytes.chunks_exact(8).enumerate() {
        limbs[3 - i] = u64::from_be_bytes(chunk.try_into().unwrap());
    }
    
    BigInteger256::new(limbs)
}

pub fn generate_g1_point(index: usize) -> [U256; 2] {
    // Protect against index 0 which gives point at infinity
    let scalar_val = if index == 0 {
        index + 1
    } else {
        index
    };
    let scalar = Fr::from(scalar_val as u64);
    
    // Use the precomputed generator
    let point = G1Affine::from((*G1_GENERATOR_PROJ) * scalar);
    
    // Convert G1Affine to [U256; 2] format
    let x_bigint: BigInteger256 = point.x.into();
    let y_bigint: BigInteger256 = point.y.into();
    
    // Optimized byte extraction using fixed-size arrays (no heap allocation)
    let mut x_bytes = [0u8; 32];
    let mut y_bytes = [0u8; 32];
    
    // Fill bytes from limbs (big-endian)
    // BigInteger256 stores limbs in little-endian order (limbs[0] = least significant)
    // So we need to reverse to get big-endian bytes
    for (i, &limb) in x_bigint.as_ref().iter().enumerate() {
        x_bytes[(3 - i)*8..(4 - i)*8].copy_from_slice(&limb.to_be_bytes());
    }
    for (i, &limb) in y_bigint.as_ref().iter().enumerate() {
        y_bytes[(3 - i)*8..(4 - i)*8].copy_from_slice(&limb.to_be_bytes());
    }
    
    [
        U256::from_be_slice(&x_bytes),
        U256::from_be_slice(&y_bytes),
    ]
}

pub fn generate_g2_point(index: usize) -> [U256; 4] {
    // Protect against index 0 which gives point at infinity
    let scalar_val = if index == 0 {
        index + 1
    } else {
        index
    };
    let scalar = Fr::from(scalar_val as u64);
    
    // Use the precomputed generator
    let point = G2Affine::from((*G2_GENERATOR_PROJ) * scalar);
    
    // Convert G2Affine to [U256; 4] format
    let x = point.x;
    let y = point.y;
    
    let x0_bigint: BigInteger256 = x.c0.into();
    let x1_bigint: BigInteger256 = x.c1.into();
    let y0_bigint: BigInteger256 = y.c0.into();
    let y1_bigint: BigInteger256 = y.c1.into();
    
    // Optimized byte extraction using fixed-size arrays (no heap allocation)
    let mut x0_bytes = [0u8; 32];
    let mut x1_bytes = [0u8; 32];
    let mut y0_bytes = [0u8; 32];
    let mut y1_bytes = [0u8; 32];
    
    // Fill bytes from limbs (big-endian)
    // BigInteger256 stores limbs in little-endian order (limbs[0] = least significant)
    // So we need to reverse to get big-endian bytes
    for (i, &limb) in x0_bigint.as_ref().iter().enumerate() {
        x0_bytes[(3 - i)*8..(4 - i)*8].copy_from_slice(&limb.to_be_bytes());
    }
    for (i, &limb) in x1_bigint.as_ref().iter().enumerate() {
        x1_bytes[(3 - i)*8..(4 - i)*8].copy_from_slice(&limb.to_be_bytes());
    }
    for (i, &limb) in y0_bigint.as_ref().iter().enumerate() {
        y0_bytes[(3 - i)*8..(4 - i)*8].copy_from_slice(&limb.to_be_bytes());
    }
    for (i, &limb) in y1_bigint.as_ref().iter().enumerate() {
        y1_bytes[(3 - i)*8..(4 - i)*8].copy_from_slice(&limb.to_be_bytes());
    }
    
    [
        U256::from_be_slice(&x0_bytes),
        U256::from_be_slice(&x1_bytes),
        U256::from_be_slice(&y0_bytes),
        U256::from_be_slice(&y1_bytes),
    ]
}