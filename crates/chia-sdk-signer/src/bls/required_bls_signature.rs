use chia_bls::PublicKey;
use chia_consensus::make_aggsig_final_message::u64_to_bytes;
use chia_protocol::{Bytes, Bytes32, Coin};
use chia_sdk_types::conditions::{AggSig, AggSigKind};

use crate::{MAX_AGG_SIG_MESSAGE_LEN, SignerError};

use super::AggSigConstants;

#[derive(Debug, Clone)]
pub struct RequiredBlsSignature {
    pub public_key: PublicKey,
    pub raw_message: Bytes,
    pub appended_info: Vec<u8>,
    pub domain_string: Option<Bytes32>,
}

impl RequiredBlsSignature {
    /// Converts an [`AggSig`] condition to a `RequiredBlsSignature`, rejecting
    /// conditions that consensus would consider invalid.
    ///
    /// This mirrors the checks in `chia_consensus::conditions`:
    /// * Messages longer than [`MAX_AGG_SIG_MESSAGE_LEN`] bytes are rejected.
    /// * `AGG_SIG_UNSAFE` messages that end with any of the `AGG_SIG_*` additional data
    ///   values are rejected, since signing them would produce a signature that is also valid
    ///   for another kind of `AGG_SIG_*` condition (for example, `AGG_SIG_ME` on another coin).
    pub fn try_from_condition(
        coin: &Coin,
        condition: AggSig,
        constants: &AggSigConstants,
    ) -> Result<Self, SignerError> {
        Self::validate_condition(&condition, constants)?;
        Ok(Self::from_condition(coin, condition, constants))
    }

    /// Checks that an [`AggSig`] condition would be accepted by consensus.
    /// See [`RequiredBlsSignature::try_from_condition`] for details.
    pub fn validate_condition(
        condition: &AggSig,
        constants: &AggSigConstants,
    ) -> Result<(), SignerError> {
        let message = condition.message.as_ref();

        if message.len() > MAX_AGG_SIG_MESSAGE_LEN {
            return Err(SignerError::AggSigMessageTooLong(message.len()));
        }

        if condition.kind == AggSigKind::Unsafe
            && constants
                .all()
                .iter()
                .any(|additional_data| message.ends_with(additional_data.as_ref()))
        {
            return Err(SignerError::UnsafeMessageHasAdditionalData);
        }

        Ok(())
    }

    /// Converts a known [`AggSig`] condition to a `RequiredSignature` if possible.
    ///
    /// This does not check that the condition is valid according to consensus.
    /// Prefer [`RequiredBlsSignature::try_from_condition`] for untrusted conditions.
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
                };
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

#[cfg(test)]
mod tests {
    use chia_bls::SecretKey;
    use chia_sdk_types::{MAINNET_CONSTANTS, TESTNET11_CONSTANTS};

    use super::*;

    const ALL_KINDS: [AggSigKind; 8] = [
        AggSigKind::Parent,
        AggSigKind::Puzzle,
        AggSigKind::Amount,
        AggSigKind::PuzzleAmount,
        AggSigKind::ParentAmount,
        AggSigKind::ParentPuzzle,
        AggSigKind::Unsafe,
        AggSigKind::Me,
    ];

    fn coin() -> Coin {
        Coin::new(Bytes32::from([1; 32]), Bytes32::from([2; 32]), 3)
    }

    fn agg_sig(kind: AggSigKind, message: Vec<u8>) -> AggSig {
        AggSig::new(
            kind,
            SecretKey::from_seed(&[7; 32]).public_key(),
            message.into(),
        )
    }

    fn all_constants() -> Vec<AggSigConstants> {
        vec![
            AggSigConstants::from(&*MAINNET_CONSTANTS),
            AggSigConstants::from(&*TESTNET11_CONSTANTS),
            AggSigConstants::new(Bytes32::from([9; 32])),
        ]
    }

    #[test]
    fn test_unsafe_rejects_every_additional_data_suffix() {
        for constants in all_constants() {
            for additional_data in constants.all() {
                // Exactly the additional data
                let condition = agg_sig(AggSigKind::Unsafe, additional_data.to_vec());
                assert!(matches!(
                    RequiredBlsSignature::try_from_condition(&coin(), condition, &constants),
                    Err(SignerError::UnsafeMessageHasAdditionalData)
                ));

                // A message crafted to collide with another AGG_SIG kind,
                // such as AGG_SIG_ME for a different coin
                let mut message = b"hello".to_vec();
                message.extend(coin().coin_id().to_bytes());
                message.extend(additional_data.to_bytes());
                let condition = agg_sig(AggSigKind::Unsafe, message);
                assert!(matches!(
                    RequiredBlsSignature::validate_condition(&condition, &constants),
                    Err(SignerError::UnsafeMessageHasAdditionalData)
                ));
            }
        }
    }

    #[test]
    fn test_unsafe_allows_other_messages() {
        for constants in all_constants() {
            let me = constants.me().to_vec();

            let cases = vec![
                Vec::new(),
                vec![1, 2, 3],
                // Shorter than any additional data
                me[1..].to_vec(),
                // Additional data that isn't at the end of the message
                [me.clone(), vec![0]].concat(),
                // Additional data with the last byte changed
                [&me[..31], &[me[31] ^ 1]].concat(),
                vec![0; MAX_AGG_SIG_MESSAGE_LEN],
            ];

            for message in cases {
                let condition = agg_sig(AggSigKind::Unsafe, message.clone());
                let required =
                    RequiredBlsSignature::try_from_condition(&coin(), condition, &constants)
                        .unwrap();
                assert_eq!(required.message(), message);
                assert_eq!(required.domain_string, None);
            }
        }
    }

    #[test]
    fn test_unsafe_suffix_from_other_network_is_allowed() {
        // Consensus only checks the additional data of the network it's running on
        let mainnet = AggSigConstants::from(&*MAINNET_CONSTANTS);
        let testnet = AggSigConstants::from(&*TESTNET11_CONSTANTS);
        let condition = agg_sig(AggSigKind::Unsafe, testnet.me().to_vec());
        assert!(RequiredBlsSignature::validate_condition(&condition, &mainnet).is_ok());
        assert!(RequiredBlsSignature::validate_condition(&condition, &testnet).is_err());
    }

    #[test]
    fn test_other_kinds_allow_additional_data_suffix() {
        // Only AGG_SIG_UNSAFE needs this check, since the other kinds
        // always have their own additional data appended
        let constants = AggSigConstants::from(&*MAINNET_CONSTANTS);
        for kind in ALL_KINDS {
            if kind == AggSigKind::Unsafe {
                continue;
            }
            for additional_data in constants.all() {
                let condition = agg_sig(kind, additional_data.to_vec());
                assert!(RequiredBlsSignature::validate_condition(&condition, &constants).is_ok());
            }
        }
    }

    #[test]
    fn test_message_length_limit() {
        let constants = AggSigConstants::from(&*MAINNET_CONSTANTS);
        for kind in ALL_KINDS {
            let condition = agg_sig(kind, vec![0; MAX_AGG_SIG_MESSAGE_LEN]);
            assert!(RequiredBlsSignature::validate_condition(&condition, &constants).is_ok());

            let condition = agg_sig(kind, vec![0; MAX_AGG_SIG_MESSAGE_LEN + 1]);
            assert!(matches!(
                RequiredBlsSignature::try_from_condition(&coin(), condition, &constants),
                Err(SignerError::AggSigMessageTooLong(len)) if len == MAX_AGG_SIG_MESSAGE_LEN + 1
            ));
        }
    }

    #[test]
    fn test_try_from_condition_matches_from_condition() {
        let constants = AggSigConstants::from(&*MAINNET_CONSTANTS);
        for kind in ALL_KINDS {
            let condition = agg_sig(kind, vec![4, 5, 6]);
            let expected =
                RequiredBlsSignature::from_condition(&coin(), condition.clone(), &constants);
            let actual =
                RequiredBlsSignature::try_from_condition(&coin(), condition, &constants).unwrap();
            assert_eq!(actual.message(), expected.message());
            assert_eq!(actual.public_key, expected.public_key);
        }
    }

    #[test]
    fn test_all_matches_consensus_constants() {
        let c = &*MAINNET_CONSTANTS;
        assert_eq!(
            AggSigConstants::from(c).all(),
            [
                c.agg_sig_me_additional_data,
                c.agg_sig_parent_additional_data,
                c.agg_sig_puzzle_additional_data,
                c.agg_sig_amount_additional_data,
                c.agg_sig_puzzle_amount_additional_data,
                c.agg_sig_parent_amount_additional_data,
                c.agg_sig_parent_puzzle_additional_data,
            ]
        );
        assert_eq!(
            AggSigConstants::new(c.agg_sig_me_additional_data),
            AggSigConstants::from(c)
        );
    }
}
