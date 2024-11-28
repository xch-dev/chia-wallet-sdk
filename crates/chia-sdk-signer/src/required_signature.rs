use chia_bls::PublicKey;
use chia_protocol::{Bytes, Bytes32, Coin, CoinSpend};
use chia_sdk_types::{AggSig, AggSigKind, Condition};
use clvm_traits::{FromClvm, ToClvm};
use clvmr::{run_program, Allocator, ChiaDialect};

use crate::{AggSigConstants, SecpDialect, SignerError};

#[derive(Debug, Clone)]
pub struct RequiredSignature {
    public_key: PublicKey,
    raw_message: Bytes,
    appended_info: Vec<u8>,
    domain_string: Option<Bytes32>,
}

impl RequiredSignature {
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

    /// Calculates the required signatures for a coin spend.
    /// All of these signatures aggregated together should be
    /// sufficient, unless secp keys are used as well.
    pub fn from_coin_spend(
        allocator: &mut Allocator,
        coin_spend: &CoinSpend,
        constants: &AggSigConstants,
    ) -> Result<Vec<Self>, SignerError> {
        let puzzle = coin_spend.puzzle_reveal.to_clvm(allocator)?;
        let solution = coin_spend.solution.to_clvm(allocator)?;
        let dialect = SecpDialect::new(ChiaDialect::new(0));
        let output = run_program(allocator, &dialect, puzzle, solution, 11_000_000_000)?.1;
        let conditions = Vec::<Condition>::from_clvm(allocator, output)?;

        let mut result = Vec::new();

        for condition in conditions {
            let Some(agg_sig) = condition.into_agg_sig() else {
                continue;
            };

            if agg_sig.public_key.is_inf() {
                return Err(SignerError::InfinityPublicKey);
            }

            result.push(Self::from_condition(&coin_spend.coin, agg_sig, constants));
        }

        Ok(result)
    }

    /// Calculates the required signatures for a spend bundle.
    /// All of these signatures aggregated together should be
    /// sufficient, unless secp keys are used as well.
    pub fn from_coin_spends(
        allocator: &mut Allocator,
        coin_spends: &[CoinSpend],
        constants: &AggSigConstants,
    ) -> Result<Vec<Self>, SignerError> {
        let mut required_signatures = Vec::new();
        for coin_spend in coin_spends {
            required_signatures.extend(Self::from_coin_spend(allocator, coin_spend, constants)?);
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
    use chia_sdk_types::MAINNET_CONSTANTS;
    use hex_literal::hex;

    #[test]
    fn test_messages() {
        let coin = Coin::new(Bytes32::from([1; 32]), Bytes32::from([2; 32]), 3);

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
                Some(hex::encode(MAINNET_CONSTANTS.agg_sig_me_additional_data)),
            ),
            (condition!(Unsafe), String::new(), None),
            (
                condition!(Parent),
                "0101010101010101010101010101010101010101010101010101010101010101".to_string(),
                Some(
                    "baf5d69c647c91966170302d18521b0a85663433d161e72c826ed08677b53a74".to_string(),
                ),
            ),
            (
                condition!(Puzzle),
                "0202020202020202020202020202020202020202020202020202020202020202".to_string(),
                Some(
                    "284fa2ef486c7a41cc29fc99c9d08376161e93dd37817edb8219f42dca7592c4".to_string(),
                ),
            ),
            (
                condition!(ParentPuzzle),
                "0101010101010101010101010101010101010101010101010101010101010101\
0202020202020202020202020202020202020202020202020202020202020202"
                    .to_string(),
                Some(
                    "2ebfdae17b29d83bae476a25ea06f0c4bd57298faddbbc3ec5ad29b9b86ce5df".to_string(),
                ),
            ),
            (
                condition!(Amount),
                "03".to_string(),
                Some(
                    "cda186a9cd030f7a130fae45005e81cae7a90e0fa205b75f6aebc0d598e0348e".to_string(),
                ),
            ),
            (
                condition!(PuzzleAmount),
                "020202020202020202020202020202020202020202020202020202020202020203".to_string(),
                Some(
                    "0f7d90dff0613e6901e24dae59f1e690f18b8f5fbdcf1bb192ac9deaf7de22ad".to_string(),
                ),
            ),
            (
                condition!(ParentAmount),
                "010101010101010101010101010101010101010101010101010101010101010103".to_string(),
                Some(
                    "585796bd90bb553c0430b87027ffee08d88aba0162c6e1abbbcc6b583f2ae7f9".to_string(),
                ),
            ),
        ];

        let constants = AggSigConstants::from(&*MAINNET_CONSTANTS);

        for (condition, appended_info, domain_string) in cases {
            let required = RequiredSignature::from_condition(&coin, condition, &constants);

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
