use chia_bls::PublicKey;
use chia_protocol::{Bytes, Coin, CoinSpend, SpendBundle};
use clvm_traits::{FromClvm, FromClvmError};
use clvmr::{allocator::NodePtr, reduction::EvalErr, Allocator};
use sha2::{digest::FixedOutput, Digest, Sha256};
use thiserror::Error;

use crate::{AggSig, AggSigKind};

/// An error that occurs while trying to sign a coin spend.
#[derive(Debug, Error)]
pub enum ConditionError {
    /// An error that occurs while trying to calculate the conditions.
    #[error("{0:?}")]
    Eval(#[from] EvalErr),

    /// An error that occurs while attempting to parse the conditions.
    #[error("{0}")]
    Clvm(#[from] FromClvmError),
}

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
    pub fn from_condition(coin: &Coin, condition: AggSig, agg_sig_me: [u8; 32]) -> Option<Self> {
        let mut hasher = Sha256::new();
        hasher.update(agg_sig_me);

        let public_key = condition.public_key;
        let message = condition.message;

        let required_signature = match condition.kind {
            AggSigKind::Parent => {
                hasher.update([43]);
                let parent = coin.parent_coin_info;
                Self {
                    public_key,
                    raw_message: message,
                    appended_info: parent.to_vec(),
                    domain_string: Some(hasher.finalize_fixed().into()),
                }
            }
            AggSigKind::Puzzle => {
                hasher.update([44]);
                let puzzle = coin.puzzle_hash;
                Self {
                    public_key,
                    raw_message: message,
                    appended_info: puzzle.to_vec(),
                    domain_string: Some(hasher.finalize_fixed().into()),
                }
            }
            AggSigKind::Amount => {
                hasher.update([45]);
                Self {
                    public_key,
                    raw_message: message,
                    appended_info: u64_to_bytes(coin.amount),
                    domain_string: Some(hasher.finalize_fixed().into()),
                }
            }
            AggSigKind::PuzzleAmount => {
                hasher.update([46]);
                let puzzle = coin.puzzle_hash;
                Self {
                    public_key,
                    raw_message: message,
                    appended_info: [puzzle.to_vec(), u64_to_bytes(coin.amount)].concat(),
                    domain_string: Some(hasher.finalize_fixed().into()),
                }
            }
            AggSigKind::ParentAmount => {
                hasher.update([47]);
                let parent = coin.parent_coin_info;
                Self {
                    public_key,
                    raw_message: message,
                    appended_info: [parent.to_vec(), u64_to_bytes(coin.amount)].concat(),
                    domain_string: Some(hasher.finalize_fixed().into()),
                }
            }
            AggSigKind::ParentPuzzle => {
                hasher.update([48]);
                let parent = coin.parent_coin_info;
                let puzzle = coin.puzzle_hash;
                Self {
                    public_key,
                    raw_message: message,
                    appended_info: [parent.to_vec(), puzzle.to_vec()].concat(),
                    domain_string: Some(hasher.finalize_fixed().into()),
                }
            }
            AggSigKind::Unsafe => Self {
                public_key,
                raw_message: message,
                appended_info: Vec::new(),
                domain_string: None,
            },
            AggSigKind::Me => Self {
                public_key,
                raw_message: message,
                appended_info: coin.coin_id().into(),
                domain_string: Some(agg_sig_me),
            },
        };

        Some(required_signature)
    }

    /// Calculates the required signatures for a coin spend.
    /// All of these signatures are aggregated together should
    /// sufficient, unless secp keys are used as well.
    pub fn from_coin_spend(
        allocator: &mut Allocator,
        coin_spend: &CoinSpend,
        agg_sig_me: [u8; 32],
    ) -> Result<Vec<Self>, ConditionError> {
        let output = coin_spend
            .puzzle_reveal
            .run(allocator, 0, u64::MAX, &coin_spend.solution)?
            .1;

        let mut result = Vec::new();

        for condition in Vec::<NodePtr>::from_clvm(allocator, output)? {
            let agg_sig = AggSig::from_clvm(allocator, condition)?;
            if let Some(required_signature) =
                Self::from_condition(&coin_spend.coin, agg_sig, agg_sig_me)
            {
                result.push(required_signature);
            }
        }

        Ok(result)
    }

    /// Calculates the required signatures for a spend bundle.
    /// All of these signatures are aggregated together should
    /// sufficient, unless secp keys are used as well.
    pub fn from_spend_bundle(
        allocator: &mut Allocator,
        spend_bundle: &SpendBundle,
        agg_sig_me: [u8; 32],
    ) -> Result<Vec<Self>, ConditionError> {
        let mut required_signatures = Vec::new();
        for coin_spend in &spend_bundle.coin_spends {
            required_signatures.extend(Self::from_coin_spend(allocator, coin_spend, agg_sig_me)?);
        }
        Ok(required_signatures)
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

    /// The domain string that is appended to the condition's message.
    pub fn domain_string(&self) -> Option<[u8; 32]> {
        self.domain_string
    }

    /// Computes the message that needs to be signed.
    pub fn final_message(&self) -> Vec<u8> {
        let mut message = Vec::from(self.raw_message.as_ref());
        message.extend(&self.appended_info);
        if let Some(domain_string) = self.domain_string {
            message.extend(domain_string);
        }
        message
    }
}

fn u64_to_bytes(amount: u64) -> Vec<u8> {
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

#[cfg(test)]
mod tests {
    use crate::testing::SECRET_KEY;

    use super::*;

    use chia_bls::derive_keys::master_to_wallet_unhardened;
    use chia_protocol::Bytes32;
    use chia_wallet::{standard::DEFAULT_HIDDEN_PUZZLE_HASH, DeriveSynthetic};

    #[test]
    fn test_messages() {
        let coin = Coin::new(Bytes32::from([1; 32]), Bytes32::from([2; 32]), 3);
        let agg_sig_data = [4u8; 32];

        let public_key = master_to_wallet_unhardened(&SECRET_KEY.public_key(), 0)
            .derive_synthetic(&DEFAULT_HIDDEN_PUZZLE_HASH);

        let message: Bytes = vec![1, 2, 3].into();

        macro_rules! condition {
            ($variant:ident) => {
                AggSig {
                    kind: AggSigKind::$variant,
                    public_key: public_key.clone(),
                    message: message.clone(),
                }
            };
        }

        let cases = vec![
            (
                condition!(Me),
                hex::encode(coin.coin_id()),
                Some(hex::encode(agg_sig_data)),
            ),
            (condition!(Unsafe), String::new(), None),
            (
                condition!(Parent),
                "0101010101010101010101010101010101010101010101010101010101010101".to_string(),
                Some(
                    "e30fe176cb4a03044620b0644b5570d8e11f9e144bea1ad63e98c94f0a8ba104".to_string(),
                ),
            ),
            (
                condition!(Puzzle),
                "0202020202020202020202020202020202020202020202020202020202020202".to_string(),
                Some(
                    "56753940d4d262c6f36619c9f02a81e249788f3e1e7e5c5d51efef7def915d3b".to_string(),
                ),
            ),
            (
                condition!(ParentPuzzle),
                "0101010101010101010101010101010101010101010101010101010101010101\
0202020202020202020202020202020202020202020202020202020202020202"
                    .to_string(),
                Some(
                    "8374c0de21a2ee2394dda1aba8705617bb9bce71d7c483e9b5c7c883c4f5d7cb".to_string(),
                ),
            ),
            (
                condition!(Amount),
                "03".to_string(),
                Some(
                    "4adba988ab536948864fb63ed13c779a16cc00a93b50a11ebf55985f586f05b9".to_string(),
                ),
            ),
            (
                condition!(PuzzleAmount),
                "020202020202020202020202020202020202020202020202020202020202020203".to_string(),
                Some(
                    "06f2ea8543ec16347ca452086d4c5ef12e0240f1e6ed6233f961ea8eb612becb".to_string(),
                ),
            ),
            (
                condition!(ParentAmount),
                "010101010101010101010101010101010101010101010101010101010101010103".to_string(),
                Some(
                    "1e09a530a1f9fc586044116b300c0a90efa787ebcf0d6f221bbd1306f1a37a8c".to_string(),
                ),
            ),
        ];

        for (condition, appended_info, domain_string) in cases {
            let required =
                RequiredSignature::from_condition(&coin, condition, agg_sig_data).unwrap();

            assert_eq!(required.public_key(), &public_key);
            assert_eq!(required.raw_message(), message.as_ref());
            assert_eq!(hex::encode(required.appended_info()), appended_info);
            assert_eq!(required.domain_string().map(hex::encode), domain_string);

            let mut message = Vec::<u8>::new();
            message.extend(required.raw_message());
            message.extend(required.appended_info());
            if let Some(domain_string) = required.domain_string() {
                message.extend(domain_string);
            }

            assert_eq!(hex::encode(message), hex::encode(required.final_message()));
        }
    }

    #[test]
    fn test_bytes() {
        assert_eq!(u64_to_bytes(0), Vec::<u8>::new());
        assert_eq!(u64_to_bytes(1), &[1]);
        assert_eq!(u64_to_bytes(45213), &[0, 176, 157]);
        assert_eq!(
            u64_to_bytes(u64::MAX),
            &[255, 255, 255, 255, 255, 255, 255, 255]
        );
        assert_eq!(u64_to_bytes(1721349832147), &[1, 144, 200, 113, 253, 211]);
        assert_eq!(u64_to_bytes(10000), &[39, 16]);
        assert_eq!(u64_to_bytes(1000), &[3, 232]);
        assert_eq!(
            u64_to_bytes(u64::MAX - 1),
            &[255, 255, 255, 255, 255, 255, 255, 254]
        );
        assert_eq!(
            u64_to_bytes(u64::MAX / 2),
            &[127, 255, 255, 255, 255, 255, 255, 255]
        );
    }
}
