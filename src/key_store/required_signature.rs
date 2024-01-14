use chia_bls::PublicKey;
use chia_protocol::{Bytes, CoinSpend};
use clvmr::allocator::NodePtr;
use sha2::{digest::FixedOutput, Digest, Sha256};

use crate::{u64_to_bytes, Condition};

#[derive(Debug, Clone)]
pub struct RequiredSignature {
    public_key: PublicKey,
    raw_message: Bytes,
    extra_data: Vec<u8>,
    domain_string: Option<[u8; 32]>,
}

impl RequiredSignature {
    pub fn try_from_condition(
        coin_spend: &CoinSpend,
        condition: Condition<NodePtr>,
        agg_sig_me_extra_data: [u8; 32],
    ) -> Option<Self> {
        let mut hasher = Sha256::new();
        hasher.update(agg_sig_me_extra_data);

        let agg_sig_info = match condition {
            Condition::AggSigParent {
                public_key,
                message,
            } => {
                hasher.update([43]);
                let parent = &coin_spend.coin.parent_coin_info;
                RequiredSignature {
                    public_key,
                    raw_message: message,
                    extra_data: parent.to_vec(),
                    domain_string: Some(hasher.finalize_fixed().into()),
                }
            }
            Condition::AggSigPuzzle {
                public_key,
                message,
            } => {
                hasher.update([44]);
                let puzzle = &coin_spend.coin.puzzle_hash;
                RequiredSignature {
                    public_key,
                    raw_message: message,
                    extra_data: puzzle.to_vec(),
                    domain_string: Some(hasher.finalize_fixed().into()),
                }
            }
            Condition::AggSigAmount {
                public_key,
                message,
            } => {
                hasher.update([45]);
                RequiredSignature {
                    public_key,
                    raw_message: message,
                    extra_data: u64_to_bytes(coin_spend.coin.amount),
                    domain_string: Some(hasher.finalize_fixed().into()),
                }
            }
            Condition::AggSigPuzzleAmount {
                public_key,
                message,
            } => {
                hasher.update([46]);
                let puzzle = &coin_spend.coin.puzzle_hash;
                RequiredSignature {
                    public_key,
                    raw_message: message,
                    extra_data: [puzzle.to_vec(), u64_to_bytes(coin_spend.coin.amount)].concat(),
                    domain_string: Some(hasher.finalize_fixed().into()),
                }
            }
            Condition::AggSigParentAmount {
                public_key,
                message,
            } => {
                hasher.update([47]);
                let parent = &coin_spend.coin.parent_coin_info;
                RequiredSignature {
                    public_key,
                    raw_message: message,
                    extra_data: [parent.to_vec(), u64_to_bytes(coin_spend.coin.amount)].concat(),
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
                RequiredSignature {
                    public_key,
                    raw_message: message,
                    extra_data: [parent.to_vec(), puzzle.to_vec()].concat(),
                    domain_string: Some(hasher.finalize_fixed().into()),
                }
            }
            Condition::AggSigUnsafe {
                public_key,
                message,
            } => RequiredSignature {
                public_key,
                raw_message: message,
                extra_data: Vec::new(),
                domain_string: None,
            },
            Condition::AggSigMe {
                public_key,
                message,
            } => RequiredSignature {
                public_key,
                raw_message: message,
                extra_data: coin_spend.coin.coin_id().into(),
                domain_string: Some(agg_sig_me_extra_data),
            },
            _ => return None,
        };

        Some(agg_sig_info)
    }

    pub fn public_key(&self) -> &PublicKey {
        &self.public_key
    }

    pub fn raw_message(&self) -> &[u8] {
        self.raw_message.as_ref()
    }

    pub fn extra_data(&self) -> &[u8] {
        &self.extra_data
    }

    pub fn final_message(&self) -> Vec<u8> {
        let mut message = Vec::from(self.raw_message.as_ref());
        message.extend(&self.extra_data);
        if let Some(domain_string) = self.domain_string {
            message.extend(domain_string);
        }
        message
    }
}
