use chia_protocol::Bytes32;
use chia_sdk_types::puzzles::{
    Eip712PrefixAndDomainSeparator, P2_EIP712_MESSAGE_PUZZLE_HASH, P2Eip712MessageArgs,
    P2Eip712MessageSolution,
};
use chia_secp::{K1PublicKey, K1Signature};
use clvm_traits::FromClvm;
use clvmr::{Allocator, NodePtr};
use sha3::{Digest, Keccak256};

use crate::{DriverError, Layer, Puzzle, Spend, SpendContext};

/// The CHIP-0037 EIP-712 message [`Layer`].
///
/// Allows a coin to be controlled by an Ethereum-style wallet that signs an
/// EIP-712 typed-data message of type `ChiaCoinSpend(bytes32 coin_id, bytes32
/// delegated_puzzle_hash)`. The puzzle reconstructs the EIP-712 digest from
/// the signed envelope (`\x19\x01 || domainSeparator || keccak256(typeHash ||
/// coin_id || delegated_puzzle_hash)`) inside a `softfork` guard so the spend
/// is bound to this exact coin and chosen `delegated_puzzle`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct P2Eip712MessageLayer {
    pub prefix_and_domain_separator: Eip712PrefixAndDomainSeparator,
    pub public_key: K1PublicKey,
}

impl P2Eip712MessageLayer {
    pub fn new(
        public_key: K1PublicKey,
        prefix_and_domain_separator: Eip712PrefixAndDomainSeparator,
    ) -> Self {
        Self {
            prefix_and_domain_separator,
            public_key,
        }
    }

    /// Convenience constructor that derives the prefix-and-domain-separator
    /// from a network's genesis challenge.
    pub fn from_genesis_challenge(public_key: K1PublicKey, genesis_challenge: Bytes32) -> Self {
        Self {
            prefix_and_domain_separator: Self::prefix_and_domain_separator(genesis_challenge),
            public_key,
        }
    }

    /// Compute the EIP-712 domain separator for the given Chia network.
    ///
    /// Domain matches the schema mandated by CHIP-0037:
    /// `{ name: "Chia Coin Spend", version: "1", salt: <genesis_challenge> }`.
    pub fn domain_separator(genesis_challenge: Bytes32) -> Bytes32 {
        let type_hash = Keccak256::digest(b"EIP712Domain(string name,string version,bytes32 salt)");

        let mut to_hash = Vec::new();
        to_hash.extend_from_slice(&type_hash);
        to_hash.extend_from_slice(&Keccak256::digest(b"Chia Coin Spend"));
        to_hash.extend_from_slice(&Keccak256::digest(b"1"));
        to_hash.extend_from_slice(&genesis_challenge);

        Bytes32::new(Keccak256::digest(&to_hash).into())
    }

    /// Compute the 34-byte concatenation of the EIP-712 prefix (`\x19\x01`)
    /// and the domain separator for a given network.
    pub fn prefix_and_domain_separator(
        genesis_challenge: Bytes32,
    ) -> Eip712PrefixAndDomainSeparator {
        let mut bytes = [0u8; 34];
        bytes[0] = 0x19;
        bytes[1] = 0x01;
        bytes[2..].copy_from_slice(&Self::domain_separator(genesis_challenge));
        bytes.into()
    }

    /// Keccak-256 hash of the canonical CHIP-0037 EIP-712 type signature.
    pub fn type_hash() -> Bytes32 {
        Bytes32::new(
            Keccak256::digest(b"ChiaCoinSpend(bytes32 coin_id,bytes32 delegated_puzzle_hash)")
                .into(),
        )
    }

    /// Compute the 32-byte EIP-712 digest the off-chain wallet must sign.
    ///
    /// `digest = keccak256(prefix_and_domain || keccak256(type_hash || coin_id
    /// || delegated_puzzle_hash))`
    pub fn hash_to_sign(&self, coin_id: Bytes32, delegated_puzzle_hash: Bytes32) -> Bytes32 {
        Self::compute_hash_to_sign(
            &self.prefix_and_domain_separator,
            coin_id,
            delegated_puzzle_hash,
        )
    }

    /// Static variant of [`Self::hash_to_sign`] that doesn't require a layer
    /// instance (and therefore doesn't need a public key); useful for callers
    /// that already hold the curried `prefix_and_domain_separator` directly,
    /// such as bindings or off-chain helpers.
    pub fn compute_hash_to_sign(
        prefix_and_domain_separator: &Eip712PrefixAndDomainSeparator,
        coin_id: Bytes32,
        delegated_puzzle_hash: Bytes32,
    ) -> Bytes32 {
        let mut to_hash = Vec::with_capacity(96);
        to_hash.extend_from_slice(&Self::type_hash());
        to_hash.extend_from_slice(&coin_id);
        to_hash.extend_from_slice(&delegated_puzzle_hash);
        let message_hash = Keccak256::digest(&to_hash);

        let mut to_hash = Vec::with_capacity(34 + 32);
        to_hash.extend_from_slice(prefix_and_domain_separator);
        to_hash.extend_from_slice(&message_hash);

        Bytes32::new(Keccak256::digest(&to_hash).into())
    }

    /// Construct a `Spend` for a coin locked by this layer using a previously
    /// obtained signature over `hash_to_sign(coin_id, tree_hash(delegated))`.
    pub fn spend(
        &self,
        ctx: &mut SpendContext,
        my_id: Bytes32,
        signature: K1Signature,
        delegated_spend: Spend,
    ) -> Result<Spend, DriverError> {
        let signed_hash = self.hash_to_sign(my_id, ctx.tree_hash(delegated_spend.puzzle).into());
        self.construct_spend(
            ctx,
            P2Eip712MessageSolution {
                my_id,
                signed_hash,
                signature,
                delegated_puzzle: delegated_spend.puzzle,
                delegated_solution: delegated_spend.solution,
            },
        )
    }
}

impl Layer for P2Eip712MessageLayer {
    type Solution = P2Eip712MessageSolution<NodePtr, NodePtr>;

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != P2_EIP712_MESSAGE_PUZZLE_HASH {
            return Ok(None);
        }

        let args = P2Eip712MessageArgs::from_clvm(allocator, puzzle.args)?;

        if args.type_hash != Self::type_hash() {
            return Ok(None);
        }

        Ok(Some(Self {
            prefix_and_domain_separator: args.prefix_and_domain_separator,
            public_key: args.public_key,
        }))
    }

    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        Ok(P2Eip712MessageSolution::from_clvm(allocator, solution)?)
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        ctx.curry(P2Eip712MessageArgs {
            prefix_and_domain_separator: self.prefix_and_domain_separator,
            type_hash: Self::type_hash(),
            public_key: self.public_key,
        })
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        ctx.alloc(&solution)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use chia_consensus::consensus_constants::TEST_CONSTANTS;
    use chia_protocol::Bytes;
    use chia_sdk_test::Simulator;
    use chia_sdk_types::Conditions;
    use chia_secp::K1SecretKey;
    use clvm_traits::{ToClvm, clvm_quote};
    use clvm_utils::ToTreeHash;
    use clvmr::chia_dialect::ENABLE_KECCAK_OPS_OUTSIDE_GUARD;
    use clvmr::reduction::Reduction;
    use clvmr::serde::node_from_bytes;
    use hex_literal::hex;
    use rand::{Rng, SeedableRng};
    use rand_chacha::ChaCha8Rng;
    use rstest::rstest;

    fn k1_pair(seed: u64) -> (K1SecretKey, K1PublicKey) {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let sk = K1SecretKey::from_bytes(&rng.random()).unwrap();
        let pk = sk.public_key();
        (sk, pk)
    }

    #[test]
    fn test_type_hash() {
        assert_eq!(
            P2Eip712MessageLayer::type_hash(),
            Bytes32::new(hex!(
                "72930978f119c79f9de7a13bd50c9b3261132d7b4819bdf0d3ca4d4c37ade070"
            ))
        );
    }

    #[test]
    fn test_domain_separator_is_deterministic() {
        // Two invocations with the same genesis challenge must produce the
        // same domain separator (the value itself depends on which network
        // `TEST_CONSTANTS` currently models).
        let a = P2Eip712MessageLayer::domain_separator(TEST_CONSTANTS.genesis_challenge);
        let b = P2Eip712MessageLayer::domain_separator(TEST_CONSTANTS.genesis_challenge);
        assert_eq!(a, b);
    }

    #[test]
    fn test_domain_separator_mainnet_fixture() {
        // Mainnet genesis challenge per chia-blockchain/initial-config.yaml.
        let mainnet_genesis = Bytes32::new(hex!(
            "ccd5bb71183532bff220ba46c268991a3ff07eb358e8255a65c30a2dce0e5fbb"
        ));
        // Pinned digest produced by the canonical EIP-712 algorithm
        // (`keccak256(typeHash || keccak256("Chia Coin Spend") ||
        // keccak256("1") || mainnet_genesis)`); regenerate this if upstream
        // ever changes the EIP-712 schema.
        assert_eq!(
            P2Eip712MessageLayer::domain_separator(mainnet_genesis),
            Bytes32::new(hex!(
                "38d765d3bce341eed11f92fc1311d575f34cc9ee0fc0e1f03820e11aebf6b5b2"
            ))
        );
    }

    /// Pin the cost of the keccak256-reconstruction sub-puzzle that runs inside
    /// the `softfork` guard. CHIP-0037 fixes this at exactly 2605 cost.
    #[test]
    fn test_softfork_cost() -> anyhow::Result<()> {
        let mut ctx = SpendContext::new();
        // Equivalent CLVM source:
        //   (mod (PREFIX_AND_DOMAIN_SEPARATOR TYPE_HASH my_id delegated_puzzle_hash signed_hash)
        //     (if (= (keccak256 PREFIX_AND_DOMAIN_SEPARATOR
        //                       (keccak256 TYPE_HASH my_id delegated_puzzle_hash))
        //            signed_hash) () (x)))
        let puzzle_bytes =
            hex!("ff02ffff03ffff09ffff3eff02ffff3eff05ff0bff178080ff2f80ff80ffff01ff088080ff0180");
        let puzzle_ptr = node_from_bytes(&mut ctx, puzzle_bytes.as_slice())?;
        let solution_ptr = vec![
            // PREFIX_AND_DOMAIN_SEPARATOR (testnet/mainnet-independent fixture)
            Bytes::new(
                hex!("1901098ccd7d09a29365582c3f7590712bc2c2eb8503586f8a4c628c61c73ffbe4aa")
                    .to_vec(),
            ),
            // TYPE_HASH
            Bytes::new(
                hex!("72930978f119c79f9de7a13bd50c9b3261132d7b4819bdf0d3ca4d4c37ade070").to_vec(),
            ),
            // my_id
            Bytes::new(
                hex!("5c777c45fd52a17a54e420742cadc56172847d9a106ff0ff8af38ef757d84829").to_vec(),
            ),
            // delegated_puzzle_hash
            Bytes::new(
                hex!("d842dfa1453a130a8be66bc32708a2d1884662d7daaa4aae530be3259fa6712f").to_vec(),
            ),
            // signed_hash
            Bytes::new(
                hex!("9f61fdf6077c3eeb96eaa4dd450b11ba3ae17746a2c304388218137972c7ba4c").to_vec(),
            ),
        ]
        .to_clvm(&mut *ctx)?;

        let Reduction(cost, _) = clvmr::run_program(
            &mut ctx,
            &clvmr::ChiaDialect::new(ENABLE_KECCAK_OPS_OUTSIDE_GUARD),
            puzzle_ptr,
            solution_ptr,
            11_000_000_000,
        )?;

        assert_eq!(cost, 2605);
        Ok(())
    }

    #[rstest]
    #[case::successful_spend(true)]
    #[case::incorrect_signed_hash(false)]
    fn test_p2_eip712_message(#[case] correct_signed_hash: bool) -> anyhow::Result<()> {
        let (sk, pk) = k1_pair(0xC0DE_F00D);

        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();
        let ctx = &mut ctx;

        let layer =
            P2Eip712MessageLayer::from_genesis_challenge(pk, TEST_CONSTANTS.genesis_challenge);
        let coin_puzzle_reveal = layer.construct_puzzle(ctx)?;
        let coin_puzzle_hash = ctx.tree_hash(coin_puzzle_reveal);

        let coin = sim.new_coin(coin_puzzle_hash.into(), 1337);

        let delegated_puzzle_ptr =
            clvm_quote!(Conditions::new().reserve_fee(1337)).to_clvm(&mut **ctx)?;
        let delegated_solution_ptr = ctx.nil();

        // For the negative case we substitute a digest the wallet would never
        // produce for this spend, so the on-chain reconstruction check fails.
        let signed_hash: Bytes32 = if correct_signed_hash {
            layer.hash_to_sign(coin.coin_id(), ctx.tree_hash(delegated_puzzle_ptr).into())
        } else {
            layer
                .hash_to_sign(coin.coin_id(), ctx.tree_hash(delegated_puzzle_ptr).into())
                .tree_hash()
                .into()
        };

        let signed_hash_bytes: [u8; 32] = signed_hash.into();
        let signature = sk.sign_prehashed(&signed_hash_bytes)?;

        let coin_spend = layer.construct_coin_spend(
            ctx,
            coin,
            P2Eip712MessageSolution {
                my_id: coin.coin_id(),
                signed_hash,
                signature,
                delegated_puzzle: delegated_puzzle_ptr,
                delegated_solution: delegated_solution_ptr,
            },
        )?;

        ctx.insert(coin_spend);

        if correct_signed_hash {
            sim.spend_coins(ctx.take(), &[])?;
        } else {
            let err = sim
                .spend_coins(ctx.take(), &[])
                .expect_err("spend should fail when signed_hash is wrong");
            let msg = err.to_string();
            assert!(
                msg.contains("clvm raise") || msg.to_lowercase().contains("raise"),
                "unexpected error: {msg}"
            );
        }

        Ok(())
    }
}
