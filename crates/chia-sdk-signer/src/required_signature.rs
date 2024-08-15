use chia_bls::PublicKey;
use chia_protocol::{Bytes, Bytes32, Coin, CoinSpend};
use chia_sdk_types::conditions::{puzzle_conditions, AggSig, AggSigKind, Condition};
use clvm_traits::ToClvm;
use clvmr::{sha2::Sha256, Allocator};

use crate::SignerError;

#[derive(Debug, Clone)]
pub struct RequiredSignature {
    public_key: PublicKey,
    raw_message: Bytes,
    appended_info: Vec<u8>,
    domain_string: Option<Bytes32>,
}

impl RequiredSignature {
    /// Converts a known [`AggSig`] condition to a `RequiredSignature` if possible.
    pub fn from_condition(coin: &Coin, condition: AggSig, agg_sig_me: Bytes32) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(agg_sig_me);

        let public_key = condition.public_key;
        let message = condition.message;

        let appended_info = match condition.kind {
            AggSigKind::Parent => {
                hasher.update([43]);
                coin.parent_coin_info.to_vec()
            }
            AggSigKind::Puzzle => {
                hasher.update([44]);
                coin.puzzle_hash.to_vec()
            }
            AggSigKind::Amount => {
                hasher.update([45]);
                u64_to_bytes(coin.amount)
            }
            AggSigKind::PuzzleAmount => {
                hasher.update([46]);
                let puzzle = coin.puzzle_hash;
                [puzzle.to_vec(), u64_to_bytes(coin.amount)].concat()
            }
            AggSigKind::ParentAmount => {
                hasher.update([47]);
                let parent = coin.parent_coin_info;
                [parent.to_vec(), u64_to_bytes(coin.amount)].concat()
            }
            AggSigKind::ParentPuzzle => {
                hasher.update([48]);
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
                return Self {
                    public_key,
                    raw_message: message,
                    appended_info: coin.coin_id().into(),
                    domain_string: Some(agg_sig_me),
                };
            }
        };

        Self {
            public_key,
            raw_message: message,
            appended_info,
            domain_string: Some(Bytes32::new(hasher.finalize())),
        }
    }

    /// Calculates the required signatures for a coin spend.
    /// All of these signatures aggregated together should be
    /// sufficient, unless secp keys are used as well.
    pub fn from_coin_spend(
        allocator: &mut Allocator,
        coin_spend: &CoinSpend,
        agg_sig_me: Bytes32,
    ) -> Result<Vec<Self>, SignerError> {
        let puzzle = coin_spend.puzzle_reveal.to_clvm(allocator)?;
        let solution = coin_spend.solution.to_clvm(allocator)?;
        let conditions = puzzle_conditions(allocator, puzzle, solution)?;

        let mut result = Vec::new();

        for condition in conditions {
            let Condition::AggSig(agg_sig) = condition else {
                continue;
            };

            if agg_sig.public_key.is_inf() {
                return Err(SignerError::InfinityPublicKey);
            }

            result.push(Self::from_condition(&coin_spend.coin, agg_sig, agg_sig_me));
        }

        Ok(result)
    }

    /// Calculates the required signatures for a spend bundle.
    /// All of these signatures aggregated together should be
    /// sufficient, unless secp keys are used as well.
    pub fn from_coin_spends(
        allocator: &mut Allocator,
        coin_spends: &[CoinSpend],
        agg_sig_me: Bytes32,
    ) -> Result<Vec<Self>, SignerError> {
        let mut required_signatures = Vec::new();
        for coin_spend in coin_spends {
            required_signatures.extend(Self::from_coin_spend(allocator, coin_spend, agg_sig_me)?);
        }
        Ok(required_signatures)
    }

    /// The public key required to verify the signature.
    pub fn public_key(&self) -> PublicKey {
        self.public_key
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
    pub fn domain_string(&self) -> Option<Bytes32> {
        self.domain_string
    }

    /// Computes the message that needs to be signed.
    pub fn final_message(&self) -> Vec<u8> {
        let mut message = Vec::from(self.raw_message.as_ref());
        message.extend(&self.appended_info);
        if let Some(domain_string) = self.domain_string {
            message.extend(domain_string.to_bytes());
        }
        message
    }
}

fn u64_to_bytes(value: u64) -> Vec<u8> {
    let mut allocator = Allocator::new();
    let atom = allocator.new_number(value.into()).unwrap();
    allocator.atom(atom).as_ref().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    use chia_bls::{master_to_wallet_unhardened, SecretKey};
    use chia_protocol::Bytes32;
    use chia_puzzles::DeriveSynthetic;
    use hex_literal::hex;

    #[test]
    fn test_messages() {
        let coin = Coin::new(Bytes32::from([1; 32]), Bytes32::from([2; 32]), 3);
        let agg_sig_data = Bytes32::new([4u8; 32]);

        let root_sk = SecretKey::from_bytes(&hex!(
            "1b72f8ed55860ea5441729c8e36ce1d6f4c8be9bbcf658502a7a0169f55638b9"
        ))
        .unwrap();
        let public_key = master_to_wallet_unhardened(&root_sk.public_key(), 0).derive_synthetic();

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
            let required = RequiredSignature::from_condition(&coin, condition, agg_sig_data);

            assert_eq!(required.public_key(), public_key);
            assert_eq!(required.raw_message(), message.as_ref());
            assert_eq!(hex::encode(required.appended_info()), appended_info);
            assert_eq!(required.domain_string().map(hex::encode), domain_string);

            let mut message = Vec::<u8>::new();
            message.extend(required.raw_message());
            message.extend(required.appended_info());
            if let Some(domain_string) = required.domain_string() {
                message.extend(domain_string.to_bytes());
            }

            assert_eq!(hex::encode(message), hex::encode(required.final_message()));
        }
    }
}
