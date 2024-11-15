use alloy::{primitives::Address, signers::local::PrivateKeySigner};

pub trait SignerRegistry<Index: Ord> {
    fn get_signer(&self, idx: Index) -> Option<&PrivateKeySigner>;
}

pub struct SignerStore {
    signers: Vec<PrivateKeySigner>,
}

impl SignerRegistry<usize> for SignerStore {
    fn get_signer(&self, idx: usize) -> Option<&PrivateKeySigner> {
        self.signers.get(idx)
    }
}

impl SignerStore {
    pub fn new() -> Self {
        SignerStore {
            signers: Vec::new(),
        }
    }

    pub fn new_random(num_signers: usize) -> Self {
        let signers: Vec<PrivateKeySigner> = (0..num_signers)
            .map(|_| PrivateKeySigner::random())
            .collect();
        SignerStore { signers }
    }

    pub fn add_signer(&mut self, signer: PrivateKeySigner) {
        self.signers.push(signer);
    }

    pub fn remove_signer(&mut self, idx: usize) {
        self.signers.remove(idx);
    }

    pub fn get_address(&self, idx: usize) -> Option<Address> {
        self.signers.get(idx).map(|s| s.address())
    }
}
