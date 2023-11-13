use chia_bls::{
    derive_keys::master_to_wallet_unhardened_intermediate, sign, DerivableKey, PublicKey,
    SecretKey, Signature,
};
use chia_protocol::CoinSpend;
use chia_wallet::{standard::DEFAULT_HIDDEN_PUZZLE_HASH, DeriveSynthetic};
use clvm_traits::Result;
use clvmr::Allocator;

use crate::{sign_coin_spend, sign_coin_spends, KeyPair, KeyStore, PartialSignature, Signer};

pub struct SyntheticKeyStore {
    intermediate_key: SecretKey,
    hidden_puzzle_hash: [u8; 32],
    key_pairs: Vec<KeyPair>,
}

impl SyntheticKeyStore {
    pub fn new(root_key: &SecretKey) -> Self {
        Self {
            intermediate_key: master_to_wallet_unhardened_intermediate(root_key),
            hidden_puzzle_hash: DEFAULT_HIDDEN_PUZZLE_HASH,
            key_pairs: Vec::new(),
        }
    }

    pub fn with_hidden_puzzle_hash(mut self, hidden_puzzle_hash: [u8; 32]) -> Self {
        self.hidden_puzzle_hash = hidden_puzzle_hash;
        self
    }
}

impl KeyStore for SyntheticKeyStore {
    fn next_derivation_index(&self) -> u32 {
        self.key_pairs.len() as u32
    }

    fn derive_keys(&mut self, count: u32) {
        let next = self.next_derivation_index();
        for index in next..(next + count) {
            let secret_key = self
                .intermediate_key
                .derive_unhardened(index)
                .derive_synthetic(&self.hidden_puzzle_hash);
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

impl Signer for SyntheticKeyStore {
    fn sign_message(&self, index: u32, message: &[u8]) -> Signature {
        let secret_key = &self.key_pairs[index as usize].secret_key;
        sign(secret_key, message)
    }

    fn sign_coin_spend(
        &self,
        allocator: &mut Allocator,
        coin_spend: &CoinSpend,
        agg_sig_me_extra_data: [u8; 32],
    ) -> Result<PartialSignature> {
        sign_coin_spend(
            allocator,
            coin_spend,
            &self.key_pairs,
            agg_sig_me_extra_data,
        )
    }

    fn sign_coin_spends(
        &self,
        allocator: &mut clvmr::Allocator,
        coin_spends: &[chia_protocol::CoinSpend],
        agg_sig_me_extra_data: [u8; 32],
    ) -> Result<PartialSignature> {
        sign_coin_spends(
            allocator,
            coin_spends,
            &self.key_pairs,
            agg_sig_me_extra_data,
        )
    }
}
