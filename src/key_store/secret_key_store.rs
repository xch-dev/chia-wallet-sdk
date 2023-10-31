use chia_bls::{
    derive_keys::master_to_wallet_unhardened_intermediate, DerivableKey, PublicKey, SecretKey,
};

use super::KeyStore;

pub struct SecretKeyStore {
    intermediate_key: SecretKey,
}

impl SecretKeyStore {
    pub fn from_root_key(root_key: &SecretKey) -> Self {
        Self::from_intermediate_key(master_to_wallet_unhardened_intermediate(root_key))
    }

    pub fn from_intermediate_key(intermediate_key: SecretKey) -> Self {
        Self { intermediate_key }
    }
}

impl KeyStore for SecretKeyStore {
    fn public_key(&self, index: u32) -> PublicKey {
        self.intermediate_key.derive_unhardened(index).public_key()
    }
}
