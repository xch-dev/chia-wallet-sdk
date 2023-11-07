use chia_bls::{
    derive_keys::master_to_wallet_unhardened_intermediate, sign, DerivableKey, PublicKey,
    SecretKey, Signature,
};
use chia_wallet::{standard::DEFAULT_HIDDEN_PUZZLE_HASH, DeriveSynthetic};
use clvm_traits::Result;
use itertools::Itertools;

use crate::{partial_sign_coin_spends, KeyStore, Signer};

pub struct SecretKeyStore {
    intermediate_key: SecretKey,
    key_pairs: Vec<KeyPair>,
}

struct KeyPair {
    public_key: PublicKey,
    secret_key: SecretKey,
}

impl SecretKeyStore {
    pub fn new(root_key: &SecretKey) -> Self {
        Self {
            intermediate_key: master_to_wallet_unhardened_intermediate(root_key),
            key_pairs: Vec::new(),
        }
    }
}

impl KeyStore for SecretKeyStore {
    fn next_derivation_index(&self) -> u32 {
        self.key_pairs.len() as u32
    }

    fn derive_keys(&mut self, count: u32) {
        let next = self.next_derivation_index();
        for index in next..(next + count) {
            let secret_key = self
                .intermediate_key
                .derive_unhardened(index)
                .derive_synthetic(&DEFAULT_HIDDEN_PUZZLE_HASH);
            let public_key = secret_key.public_key();
            self.key_pairs.push(KeyPair {
                public_key,
                secret_key,
            });
        }
    }

    fn public_key(&self, index: u32) -> PublicKey {
        self.key_pairs[index as usize].public_key.clone()
    }
}

impl Signer for SecretKeyStore {
    fn sign_message(&self, index: u32, message: &[u8]) -> Signature {
        let secret_key = &self.key_pairs[index as usize].secret_key;
        sign(secret_key, message)
    }

    fn partial_sign_coin_spends(
        &self,
        allocator: &mut clvmr::Allocator,
        coin_spends: &[chia_protocol::CoinSpend],
        agg_sig_me_extra_data: [u8; 32],
    ) -> Result<Signature> {
        let secret_keys = self
            .key_pairs
            .iter()
            .map(|key_pair| key_pair.secret_key.clone())
            .collect_vec();

        partial_sign_coin_spends(allocator, coin_spends, &secret_keys, agg_sig_me_extra_data)
    }
}
