use chia_bls::PublicKey;
use chia_consensus::make_aggsig_final_message::u64_to_bytes;
use chia_protocol::{Bytes, Bytes32, Coin};
use chia_sdk_types::conditions::{AggSig, AggSigKind};

use super::AggSigConstants;

#[derive(Debug, Clone)]
pub struct RequiredBlsSignature {
    pub public_key: PublicKey,
    pub raw_message: Bytes,
    pub appended_info: Vec<u8>,
    pub domain_string: Option<Bytes32>,
}

impl RequiredBlsSignature {
    /// Converts a known [`AggSig`] condition to a `RequiredSignature` if possible.
    pub fn from_condition(coin: &Coin, condition: AggSig, constants: &AggSigConstants) -> Self {
        let domain_string;

        let public_key = condition.public_key;
        let message = condition.message;

        let appended_info = match condition.kind {
            AggSigKind::Parent => {
                domain_string = constants.parent();
                coin.parent_coin_info.to_vec()
            }
            AggSigKind::Puzzle => {
                domain_string = constants.puzzle();
                coin.puzzle_hash.to_vec()
            }
            AggSigKind::Amount => {
                domain_string = constants.amount();
                u64_to_bytes(coin.amount)
            }
            AggSigKind::PuzzleAmount => {
                domain_string = constants.puzzle_amount();
                let puzzle = coin.puzzle_hash;
                [puzzle.to_vec(), u64_to_bytes(coin.amount)].concat()
            }
            AggSigKind::ParentAmount => {
                domain_string = constants.parent_amount();
                let parent = coin.parent_coin_info;
                [parent.to_vec(), u64_to_bytes(coin.amount)].concat()
            }
            AggSigKind::ParentPuzzle => {
                domain_string = constants.parent_puzzle();
                [coin.parent_coin_info.to_vec(), coin.puzzle_hash.to_vec()].concat()
            }
            AggSigKind::Unsafe => {
                return Self {
                    public_key,
                    raw_message: message,
                    appended_info: Vec::new(),
                    domain_string: None,
                }
            }
            AggSigKind::Me => {
                domain_string = constants.me();
                coin.coin_id().to_vec()
            }
        };

        Self {
            public_key,
            raw_message: message,
            appended_info,
            domain_string: Some(domain_string),
        }
    }

    /// Computes the message that needs to be signed.
    pub fn message(&self) -> Vec<u8> {
        let mut message = Vec::from(self.raw_message.as_ref());
        message.extend(&self.appended_info);
        if let Some(domain_string) = self.domain_string {
            message.extend(domain_string.to_bytes());
        }
        message
    }
}
