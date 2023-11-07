use chia_bls::{sign_raw, PublicKey, SecretKey, Signature};
use chia_protocol::{Bytes, CoinSpend};
use clvm_traits::{FromClvm, Result, ToClvm};
use clvmr::{run_program, Allocator, ChiaDialect};
use itertools::Itertools;
use sha2::{
    digest::{Digest, FixedOutput},
    Sha256,
};

use crate::Condition;

mod secret_key_store;

pub use secret_key_store::*;

pub trait KeyStore: Send + Sync {
    fn next_derivation_index(&self) -> u32;
    fn derive_keys(&mut self, count: u32);
    fn public_key(&self, index: u32) -> PublicKey;

    fn derive_keys_until(&mut self, index: u32) {
        if index < self.next_derivation_index() {
            return;
        }
        self.derive_keys(index - self.next_derivation_index() + 1);
    }
}

pub trait Signer {
    fn sign_message(&self, index: u32, message: &[u8]) -> Signature;

    fn partial_sign_coin_spends(
        &self,
        allocator: &mut Allocator,
        coin_spends: &[CoinSpend],
        agg_sig_me_extra_data: [u8; 32],
    ) -> Result<Signature>;
}

struct AggSigInfo {
    public_key: PublicKey,
    message: Bytes,
    additional_info: Vec<u8>,
    domain_string: Option<[u8; 32]>,
}

pub fn partial_sign_coin_spends(
    allocator: &mut Allocator,
    coin_spends: &[CoinSpend],
    secret_keys: &[SecretKey],
    agg_sig_me_extra_data: [u8; 32],
) -> Result<Signature> {
    let mut aggregate_signature = Signature::default();
    let dialect = ChiaDialect::new(0);
    let public_keys = secret_keys
        .iter()
        .map(|secret_key| secret_key.public_key())
        .collect_vec();

    for coin_spend in coin_spends {
        let puzzle = coin_spend.puzzle_reveal.to_clvm(allocator)?;
        let solution = coin_spend.solution.to_clvm(allocator)?;
        let output = run_program(allocator, &dialect, puzzle, solution, u64::MAX)?.1;

        let conditions = Vec::<Condition>::from_clvm(allocator, output)?;
        for condition in conditions {
            let mut hasher = Sha256::new();
            hasher.update(agg_sig_me_extra_data);

            let info = match condition {
                Condition::AggSigParent {
                    public_key,
                    message,
                } => {
                    hasher.update([43]);
                    let parent = &coin_spend.coin.parent_coin_info;
                    AggSigInfo {
                        public_key,
                        message,
                        additional_info: parent.to_vec(),
                        domain_string: Some(hasher.finalize_fixed().into()),
                    }
                }
                Condition::AggSigPuzzle {
                    public_key,
                    message,
                } => {
                    hasher.update([44]);
                    let puzzle = &coin_spend.coin.puzzle_hash;
                    AggSigInfo {
                        public_key,
                        message,
                        additional_info: puzzle.to_vec(),
                        domain_string: Some(hasher.finalize_fixed().into()),
                    }
                }
                Condition::AggSigAmount {
                    public_key,
                    message,
                } => {
                    hasher.update([45]);
                    AggSigInfo {
                        public_key,
                        message,
                        additional_info: amount_to_bytes(coin_spend.coin.amount),
                        domain_string: Some(hasher.finalize_fixed().into()),
                    }
                }
                Condition::AggSigPuzzleAmount {
                    public_key,
                    message,
                } => {
                    hasher.update([46]);
                    let puzzle = &coin_spend.coin.puzzle_hash;
                    AggSigInfo {
                        public_key,
                        message,
                        additional_info: [puzzle.to_vec(), amount_to_bytes(coin_spend.coin.amount)]
                            .concat(),
                        domain_string: Some(hasher.finalize_fixed().into()),
                    }
                }
                Condition::AggSigParentAmount {
                    public_key,
                    message,
                } => {
                    hasher.update([47]);
                    let parent = &coin_spend.coin.parent_coin_info;
                    AggSigInfo {
                        public_key,
                        message,
                        additional_info: [parent.to_vec(), amount_to_bytes(coin_spend.coin.amount)]
                            .concat(),
                        domain_string: Some(hasher.finalize_fixed().into()),
                    }
                }
                Condition::AggSigParentPuzzle {
                    public_key,
                    message,
                } => {
                    hasher.update([48]);
                    let parent = &coin_spend.coin.parent_coin_info;
                    let puzzle = &coin_spend.coin.puzzle_hash;
                    AggSigInfo {
                        public_key,
                        message,
                        additional_info: [parent.to_vec(), puzzle.to_vec()].concat(),
                        domain_string: Some(hasher.finalize_fixed().into()),
                    }
                }
                Condition::AggSigUnsafe {
                    public_key,
                    message,
                } => AggSigInfo {
                    public_key,
                    message,
                    additional_info: Vec::new(),
                    domain_string: None,
                },
                Condition::AggSigMe {
                    public_key,
                    message,
                } => AggSigInfo {
                    public_key,
                    message,
                    additional_info: coin_spend.coin.coin_id().into(),
                    domain_string: Some(agg_sig_me_extra_data),
                },
                _ => continue,
            };

            let Some(index) = public_keys.iter().position(|pk| pk == &info.public_key) else {
                continue;
            };

            let mut message = Vec::from(info.message.as_ref());
            message.extend(info.additional_info);
            if let Some(domain_string) = info.domain_string {
                message.extend(domain_string);
            }

            let signature = sign_raw(&secret_keys[index], message);
            aggregate_signature.aggregate(&signature);
        }
    }

    Ok(aggregate_signature)
}

fn amount_to_bytes(amount: u64) -> Vec<u8> {
    let bytes: Vec<u8> = amount.to_be_bytes().into();
    let mut slice = bytes.as_slice();

    // Remove leading zeros.
    while (!slice.is_empty()) && (slice[0] == 0) {
        if slice.len() > 1 && (slice[1] & 0x80 == 0x80) {
            break;
        }
        slice = &slice[1..];
    }

    slice.into()
}
