use chia_protocol::{Bytes32, CoinSpend};
use chia_sdk_types::{Condition, is_debug_dialect_enabled};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::tree_hash;
use clvmr::{Allocator, ChiaDialect, ClvmFlags, dialect::Dialect, run_program};
use rue_lir::DebugDialect;
use tracing::debug;

use crate::{
    AggSigConstants, RequiredBlsSignature, RequiredSecpSignature, SecpDialect, SignerError,
};

/// The CLVM flags used to run puzzles when calculating required signatures.
/// These match `chia_sdk_types::run_puzzle` and the flags consensus uses after
/// the keccak256 operator was enabled outside of the softfork guard.
const DIALECT_FLAGS: ClvmFlags = ClvmFlags::ENABLE_KECCAK_OPS_OUTSIDE_GUARD;

/// The maximum cost allowed when running a puzzle to calculate required signatures.
const MAX_COST: u64 = 11_000_000_000;

#[derive(Debug, Clone)]
pub enum RequiredSignature {
    Bls(RequiredBlsSignature),
    Secp(RequiredSecpSignature),
}

impl RequiredSignature {
    /// Calculates the required signatures for a coin spend.
    /// All of these signatures aggregated together should be
    /// sufficient, unless secp keys are used as well.
    ///
    /// The puzzle reveal must hash to the coin's puzzle hash, otherwise
    /// [`SignerError::PuzzleHashMismatch`] is returned. This prevents a malicious
    /// spend from using a fake puzzle to request a signature that is valid for the real coin.
    pub fn from_coin_spend(
        allocator: &mut Allocator,
        coin_spend: &CoinSpend,
        constants: &AggSigConstants,
    ) -> Result<Vec<Self>, SignerError> {
        if is_debug_dialect_enabled() {
            let dialect = DebugDialect::new(DIALECT_FLAGS, false);

            let result =
                Self::from_coin_spend_with_dialect(allocator, coin_spend, constants, &dialect);

            if result.is_err() {
                let coin_id = coin_spend.coin.coin_id();
                for item in dialect.log() {
                    debug!(%coin_id, srcloc = %item.srcloc, "{}", item.message);
                }
            }

            result
        } else {
            Self::from_coin_spend_with_dialect(
                allocator,
                coin_spend,
                constants,
                &ChiaDialect::new(DIALECT_FLAGS),
            )
        }
    }

    fn from_coin_spend_with_dialect<D: Dialect>(
        allocator: &mut Allocator,
        coin_spend: &CoinSpend,
        constants: &AggSigConstants,
        dialect: &D,
    ) -> Result<Vec<Self>, SignerError> {
        let puzzle = coin_spend.puzzle_reveal.to_clvm(allocator)?;

        let puzzle_hash = Bytes32::from(tree_hash(allocator, puzzle));
        if puzzle_hash != coin_spend.coin.puzzle_hash {
            return Err(SignerError::PuzzleHashMismatch {
                expected: coin_spend.coin.puzzle_hash,
                found: puzzle_hash,
            });
        }

        let solution = coin_spend.solution.to_clvm(allocator)?;
        let dialect = SecpDialect::new(dialect);
        let output = run_program(allocator, &dialect, puzzle, solution, MAX_COST)?.1;
        let conditions = Vec::<Condition>::from_clvm(allocator, output)?;

        let mut result = Vec::new();

        for condition in conditions {
            let Some(agg_sig) = condition.into_agg_sig() else {
                continue;
            };

            if agg_sig.public_key.is_inf() {
                return Err(SignerError::InfinityPublicKey);
            }

            result.push(Self::Bls(RequiredBlsSignature::try_from_condition(
                &coin_spend.coin,
                agg_sig,
                constants,
            )?));
        }

        for item in dialect.collect() {
            result.push(Self::Secp(item));
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
}

#[cfg(test)]
mod tests {
    use super::*;

    use chia_bls::{PublicKey, SecretKey, master_to_wallet_unhardened};
    use chia_protocol::{Bytes, Coin, Program};
    use chia_puzzle_types::DeriveSynthetic;
    use chia_sdk_types::{
        MAINNET_CONSTANTS,
        conditions::{AggSig, AggSigKind},
    };
    use chia_secp::K1SecretKey;
    use clvm_traits::{clvm_list, clvm_quote};
    use clvmr::NodePtr;
    use hex_literal::hex;

    use crate::SecpPublicKey;

    const AGG_SIG_UNSAFE: u8 = 49;
    const AGG_SIG_ME: u8 = 50;
    const CREATE_COIN: u8 = 51;

    fn constants() -> AggSigConstants {
        AggSigConstants::from(&*MAINNET_CONSTANTS)
    }

    fn public_key() -> PublicKey {
        SecretKey::from_seed(&[7; 32]).public_key()
    }

    fn coin_spend(a: &Allocator, puzzle: NodePtr) -> CoinSpend {
        let puzzle_hash = tree_hash(a, puzzle).into();
        CoinSpend::new(
            Coin::new(Bytes32::from([1; 32]), puzzle_hash, 1),
            Program::from_clvm(a, puzzle).unwrap(),
            Program::default(),
        )
    }

    /// Runs the coin spend with both the debug dialect and release dialect,
    /// and checks that they agree.
    fn required(
        a: &mut Allocator,
        coin_spend: &CoinSpend,
    ) -> Result<Vec<RequiredSignature>, SignerError> {
        let release = RequiredSignature::from_coin_spend_with_dialect(
            a,
            coin_spend,
            &constants(),
            &ChiaDialect::new(DIALECT_FLAGS),
        );
        let debug = RequiredSignature::from_coin_spend(a, coin_spend, &constants());
        assert_eq!(summarize(&release), summarize(&debug));
        debug
    }

    fn summarize(
        result: &Result<Vec<RequiredSignature>, SignerError>,
    ) -> Result<Vec<String>, String> {
        match result {
            Ok(required) => Ok(required
                .iter()
                .map(|item| match item {
                    RequiredSignature::Bls(bls) => {
                        format!("{:?} {}", bls.public_key, hex::encode(bls.message()))
                    }
                    RequiredSignature::Secp(secp) => {
                        format!("{:?} {}", secp.public_key, hex::encode(secp.message_hash))
                    }
                })
                .collect()),
            Err(error) => Err(error.to_string()),
        }
    }

    fn bls(required: &[RequiredSignature]) -> Vec<&RequiredBlsSignature> {
        required
            .iter()
            .filter_map(|item| match item {
                RequiredSignature::Bls(bls) => Some(bls),
                RequiredSignature::Secp(_) => None,
            })
            .collect()
    }

    fn quoted_conditions<T: ToClvm<Allocator>>(a: &mut Allocator, conditions: T) -> NodePtr {
        clvm_quote!(conditions).to_clvm(a).unwrap()
    }

    #[test]
    fn test_agg_sig_me_for_matching_puzzle() {
        let mut a = Allocator::new();
        let message = Bytes::from(vec![1, 2, 3]);
        let puzzle = quoted_conditions(
            &mut a,
            [clvm_list!(AGG_SIG_ME, public_key(), message.clone())],
        );
        let coin_spend = coin_spend(&a, puzzle);

        let required = required(&mut a, &coin_spend).unwrap();
        let bls = bls(&required);
        assert_eq!(required.len(), 1);
        assert_eq!(bls[0].public_key, public_key());
        assert_eq!(bls[0].raw_message, message);
        assert_eq!(bls[0].appended_info, coin_spend.coin.coin_id().to_vec());
        assert_eq!(bls[0].domain_string, Some(constants().me()));
    }

    #[test]
    fn test_ignores_non_agg_sig_conditions() {
        let mut a = Allocator::new();
        let puzzle = quoted_conditions(
            &mut a,
            (
                clvm_list!(CREATE_COIN, Bytes32::from([3; 32]), 1),
                (
                    clvm_list!(AGG_SIG_UNSAFE, public_key(), Bytes::from(vec![4])),
                    (),
                ),
            ),
        );
        let coin_spend = coin_spend(&a, puzzle);

        let required = required(&mut a, &coin_spend).unwrap();
        assert_eq!(required.len(), 1);
        assert_eq!(bls(&required)[0].message(), vec![4]);
    }

    #[test]
    fn test_rejects_fake_puzzle_reveal() {
        let mut a = Allocator::new();

        // The real coin is locked by a different puzzle
        let real_puzzle = quoted_conditions(&mut a, ());
        let real_coin = coin_spend(&a, real_puzzle).coin;

        // A fake puzzle requests an AGG_SIG_ME that would be valid for the real coin
        let fake_puzzle = quoted_conditions(
            &mut a,
            [clvm_list!(AGG_SIG_ME, public_key(), Bytes::from(vec![1]))],
        );
        let mut coin_spend = coin_spend(&a, fake_puzzle);
        let fake_puzzle_hash = coin_spend.coin.puzzle_hash;
        coin_spend.coin = real_coin;

        let result = required(&mut a, &coin_spend);
        assert!(matches!(
            result,
            Err(SignerError::PuzzleHashMismatch { expected, found })
                if expected == real_coin.puzzle_hash && found == fake_puzzle_hash
        ));
    }

    #[test]
    fn test_rejects_fake_puzzle_reveal_in_bundle() {
        let mut a = Allocator::new();
        let puzzle = quoted_conditions(
            &mut a,
            [clvm_list!(AGG_SIG_ME, public_key(), Bytes::from(vec![1]))],
        );
        let valid = coin_spend(&a, puzzle);
        let mut invalid = valid.clone();
        invalid.coin.puzzle_hash = Bytes32::from([0; 32]);

        assert_eq!(
            RequiredSignature::from_coin_spends(&mut a, std::slice::from_ref(&valid), &constants())
                .unwrap()
                .len(),
            1
        );
        assert!(matches!(
            RequiredSignature::from_coin_spends(&mut a, &[valid, invalid], &constants()),
            Err(SignerError::PuzzleHashMismatch { .. })
        ));
    }

    #[test]
    fn test_rejects_infinity_public_key() {
        let mut a = Allocator::new();
        let puzzle = quoted_conditions(
            &mut a,
            [clvm_list!(
                AGG_SIG_ME,
                PublicKey::default(),
                Bytes::from(vec![1])
            )],
        );
        let coin_spend = coin_spend(&a, puzzle);
        assert!(matches!(
            required(&mut a, &coin_spend),
            Err(SignerError::InfinityPublicKey)
        ));
    }

    #[test]
    fn test_rejects_unsafe_message_with_additional_data() {
        let mut a = Allocator::new();

        // An AGG_SIG_UNSAFE that's equivalent to an AGG_SIG_ME on another coin
        let mut message = vec![1, 2, 3];
        message.extend([5; 32]);
        message.extend(constants().me().to_bytes());

        let puzzle = quoted_conditions(
            &mut a,
            [clvm_list!(
                AGG_SIG_UNSAFE,
                public_key(),
                Bytes::from(message)
            )],
        );
        let coin_spend = coin_spend(&a, puzzle);
        assert!(matches!(
            required(&mut a, &coin_spend),
            Err(SignerError::UnsafeMessageHasAdditionalData)
        ));
    }

    #[test]
    fn test_rejects_long_message() {
        let mut a = Allocator::new();
        let puzzle = quoted_conditions(
            &mut a,
            [clvm_list!(
                AGG_SIG_ME,
                public_key(),
                Bytes::from(vec![0; 1025])
            )],
        );
        let coin_spend = coin_spend(&a, puzzle);
        assert!(matches!(
            required(&mut a, &coin_spend),
            Err(SignerError::AggSigMessageTooLong(1025))
        ));
    }

    #[test]
    fn test_keccak_in_release_dialect() {
        let mut a = Allocator::new();

        // (c (c (q . 50) (c (q . pk) (c (keccak256 (q . "abc")) ()))) ())
        let puzzle = clvm_list!(
            4,
            clvm_list!(
                4,
                clvm_quote!(AGG_SIG_ME),
                clvm_list!(
                    4,
                    clvm_quote!(public_key()),
                    clvm_list!(
                        4,
                        clvm_list!(62, clvm_quote!(Bytes::from(b"abc".to_vec()))),
                        ()
                    )
                )
            ),
            ()
        )
        .to_clvm(&mut a)
        .unwrap();
        let coin_spend = coin_spend(&a, puzzle);

        let keccak_abc = hex!("4e03657aea45a94fc7d47ba826c8d667c0d1e6e33a64a036ec44f58fa12d6c45");

        let required = required(&mut a, &coin_spend).unwrap();
        assert_eq!(bls(&required)[0].raw_message.as_ref(), keccak_abc);

        // Without the keccak flag, the operator is treated as unknown and the message is wrong
        let result = RequiredSignature::from_coin_spend_with_dialect(
            &mut a,
            &coin_spend,
            &constants(),
            &ChiaDialect::new(ClvmFlags::empty()),
        );
        assert!(
            !matches!(result, Ok(ref required) if bls(required)[0].raw_message.as_ref() == keccak_abc)
        );
    }

    fn secp_puzzle(a: &mut Allocator, message_hash: [u8; 32]) -> (NodePtr, SecpPublicKey) {
        let public_key = K1SecretKey::from_bytes(&[3; 32]).unwrap().public_key();
        let verify = clvm_list!(
            Bytes::from(vec![0x13, 0xd6, 0x1f, 0x00]),
            clvm_quote!(public_key),
            clvm_quote!(Bytes32::from(message_hash)),
            clvm_quote!(Bytes::from(vec![0; 64]))
        );
        // (i (secp256k1_verify ...) (q . ()) (q . ()))
        let puzzle = clvm_list!(3, verify, clvm_quote!(()), clvm_quote!(()))
            .to_clvm(a)
            .unwrap();
        (puzzle, SecpPublicKey::K1(public_key))
    }

    #[test]
    fn test_secp_signature_collected() {
        let mut a = Allocator::new();
        let (puzzle, public_key) = secp_puzzle(&mut a, [42; 32]);
        let coin_spend = coin_spend(&a, puzzle);

        let required = required(&mut a, &coin_spend).unwrap();
        assert_eq!(required.len(), 1);
        let RequiredSignature::Secp(secp) = required[0] else {
            panic!("expected a secp signature");
        };
        assert_eq!(secp.public_key, public_key);
        assert_eq!(secp.message_hash, [42; 32]);
    }

    #[test]
    fn test_secp_rejects_fake_puzzle_reveal() {
        let mut a = Allocator::new();

        // A fake puzzle can't request a secp signature over an arbitrary hash for a real coin
        let (real_puzzle, _) = secp_puzzle(&mut a, [1; 32]);
        let real_coin = coin_spend(&a, real_puzzle).coin;
        let (fake_puzzle, _) = secp_puzzle(&mut a, [2; 32]);
        let mut coin_spend = coin_spend(&a, fake_puzzle);
        coin_spend.coin = real_coin;

        assert!(matches!(
            required(&mut a, &coin_spend),
            Err(SignerError::PuzzleHashMismatch { .. })
        ));
    }

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
            let required = RequiredBlsSignature::from_condition(&coin, condition, &constants);

            assert_eq!(required.public_key, public_key);
            assert_eq!(required.raw_message, message);
            assert_eq!(hex::encode(&required.appended_info), appended_info);
            assert_eq!(required.domain_string.map(hex::encode), domain_string);

            let mut message = Vec::<u8>::new();
            message.extend(required.raw_message.as_ref());
            message.extend(&required.appended_info);
            if let Some(domain_string) = required.domain_string {
                message.extend(domain_string.to_bytes());
            }

            assert_eq!(hex::encode(message), hex::encode(required.message()));
        }
    }
}
