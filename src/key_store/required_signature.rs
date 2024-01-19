use chia_bls::PublicKey;
use chia_protocol::{Bytes, Coin};
use clvmr::allocator::NodePtr;
use sha2::{digest::FixedOutput, Digest, Sha256};

use crate::{utils::u64_to_bytes, Condition};

/// Information about how to sign an AggSig condition.
#[derive(Debug, Clone)]
pub struct RequiredSignature {
    public_key: PublicKey,
    raw_message: Bytes,
    appended_info: Vec<u8>,
    domain_string: Option<[u8; 32]>,
}

impl RequiredSignature {
    /// Converts a known AggSig condition to a `RequiredSignature` if possible.
    pub fn try_from_condition(
        coin: &Coin,
        condition: Condition<NodePtr>,
        agg_sig_me_extra_data: [u8; 32],
    ) -> Option<Self> {
        let mut hasher = Sha256::new();
        hasher.update(agg_sig_me_extra_data);

        let required_signature = match condition {
            Condition::AggSigParent {
                public_key,
                message,
            } => {
                hasher.update([43]);
                let parent = coin.parent_coin_info;
                RequiredSignature {
                    public_key,
                    raw_message: message,
                    appended_info: parent.to_vec(),
                    domain_string: Some(hasher.finalize_fixed().into()),
                }
            }
            Condition::AggSigPuzzle {
                public_key,
                message,
            } => {
                hasher.update([44]);
                let puzzle = coin.puzzle_hash;
                RequiredSignature {
                    public_key,
                    raw_message: message,
                    appended_info: puzzle.to_vec(),
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
                    appended_info: u64_to_bytes(coin.amount),
                    domain_string: Some(hasher.finalize_fixed().into()),
                }
            }
            Condition::AggSigPuzzleAmount {
                public_key,
                message,
            } => {
                hasher.update([46]);
                let puzzle = coin.puzzle_hash;
                RequiredSignature {
                    public_key,
                    raw_message: message,
                    appended_info: [puzzle.to_vec(), u64_to_bytes(coin.amount)].concat(),
                    domain_string: Some(hasher.finalize_fixed().into()),
                }
            }
            Condition::AggSigParentAmount {
                public_key,
                message,
            } => {
                hasher.update([47]);
                let parent = coin.parent_coin_info;
                RequiredSignature {
                    public_key,
                    raw_message: message,
                    appended_info: [parent.to_vec(), u64_to_bytes(coin.amount)].concat(),
                    domain_string: Some(hasher.finalize_fixed().into()),
                }
            }
            Condition::AggSigParentPuzzle {
                public_key,
                message,
            } => {
                hasher.update([48]);
                let parent = coin.parent_coin_info;
                let puzzle = coin.puzzle_hash;
                RequiredSignature {
                    public_key,
                    raw_message: message,
                    appended_info: [parent.to_vec(), puzzle.to_vec()].concat(),
                    domain_string: Some(hasher.finalize_fixed().into()),
                }
            }
            Condition::AggSigUnsafe {
                public_key,
                message,
            } => RequiredSignature {
                public_key,
                raw_message: message,
                appended_info: Vec::new(),
                domain_string: None,
            },
            Condition::AggSigMe {
                public_key,
                message,
            } => RequiredSignature {
                public_key,
                raw_message: message,
                appended_info: coin.coin_id().into(),
                domain_string: Some(agg_sig_me_extra_data),
            },
            _ => return None,
        };

        Some(required_signature)
    }

    /// The public key required to verify the signature.
    pub fn public_key(&self) -> &PublicKey {
        &self.public_key
    }

    /// The message field of the condition, without anything appended.
    pub fn raw_message(&self) -> &[u8] {
        self.raw_message.as_ref()
    }

    /// Additional coin information that is appended to the condition's message.
    pub fn appended_info(&self) -> &[u8] {
        &self.appended_info
    }

    /// Computes the message that needs to be signed.
    pub fn message(&self) -> Vec<u8> {
        let mut message = Vec::from(self.raw_message.as_ref());
        message.extend(&self.appended_info);
        if let Some(domain_string) = self.domain_string {
            message.extend(domain_string);
        }
        message
    }
}
