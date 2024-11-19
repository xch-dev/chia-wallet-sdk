use chia_protocol::{Bytes32, BytesImpl};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, TreeHash};
use clvmr::{Allocator, NodePtr};
use hex_literal::hex;
use sha3::{Digest, Keccak256};

use crate::{DriverError, Layer, Puzzle, Spend, SpendContext};

// https://github.com/Yakuhito/hermes/blob/master/clsp/p2_eip712_message.clsp
pub const P2_EIP712_MESSAGE_PUZZLE: [u8; 276] = hex!("ff02ffff01ff02ffff03ffff22ffff20ffff8413d61f00ff17ff5fff81bf8080ffff20ffff24ffff01820ab9ffff0101ffff01ff02ffff03ffff09ffff3eff02ffff3eff05ff0bff178080ff2f80ff80ffff01ff088080ff0180ffff04ff05ffff04ff0bffff04ff2fffff04ffff02ff06ffff04ff02ffff04ff82017fff80808080ffff04ff5fff808080808080808080ffff01ff04ffff04ff04ffff04ff2fff808080ffff02ff82017fff8202ff8080ffff01ff088080ff0180ffff04ffff01ff46ff02ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff06ffff04ff02ffff04ff09ff80808080ffff02ff06ffff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ff018080");
pub const P2_EIP712_MESSAGE_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    aacce7b99db5b1e9eb16d676fa5f1a2e469ef589f29c4ab0010bac338a4df085
    "
));

pub type EthPubkeyBytes = BytesImpl<33>;
pub type EthSignatureBytes = BytesImpl<64>;
pub type Eip712PrefixAndDomainSeparator = BytesImpl<34>;

/// The p2 EIP-712 [`Layer`] allows an Ethereum wallet to control coins by signing an
/// EIP-712 message containing a delegated puzzle (for example to output [`Conditions`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct P2Eip712MessageLayer {
    pub prefix_and_domain_separator: Eip712PrefixAndDomainSeparator,
    pub pubkey: EthPubkeyBytes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct P2Eip712MessageArgs {
    pub prefix_and_domain_separator: Eip712PrefixAndDomainSeparator,
    pub type_hash: Bytes32,
    pub pubkey: EthPubkeyBytes,
}

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(solution)]
pub struct P2Eip712MessageSolution<P, S> {
    pub my_id: Bytes32,
    pub signed_hash: Bytes32,
    pub signature: EthSignatureBytes,
    pub delegated_puzzle: P,
    pub delegated_solution: S,
}

impl P2Eip712MessageLayer {
    pub fn new(
        pubkey: EthPubkeyBytes,
        prefix_and_domain_separator: Eip712PrefixAndDomainSeparator,
    ) -> Self {
        Self {
            prefix_and_domain_separator,
            pubkey,
        }
    }

    pub fn from_genesis_challenge(pubkey: EthPubkeyBytes, genesis_challenge: Bytes32) -> Self {
        Self {
            prefix_and_domain_separator: P2Eip712MessageLayer::prefix_and_domain_separator(
                genesis_challenge,
            ),
            pubkey,
        }
    }

    pub fn spend(
        &self,
        ctx: &mut SpendContext,
        my_id: Bytes32,
        signature: EthSignatureBytes,
        delegated_spend: Spend,
    ) -> Result<Spend, DriverError> {
        self.construct_spend(
            ctx,
            P2Eip712MessageSolution {
                my_id,
                signed_hash: self.hash_to_sign(my_id, ctx.tree_hash(delegated_spend.puzzle).into()),
                signature,
                delegated_puzzle: delegated_spend.puzzle,
                delegated_solution: delegated_spend.solution,
            },
        )
    }

    pub fn domain_separator(genesis_challenge: Bytes32) -> Bytes32 {
        let type_hash = Keccak256::digest(b"EIP712Domain(string name,string version,bytes32 salt)");

        let mut to_hash = Vec::new();
        to_hash.extend_from_slice(&type_hash);
        to_hash.extend_from_slice(&Keccak256::digest("Chia Coin Spend"));
        to_hash.extend_from_slice(&Keccak256::digest("1"));
        to_hash.extend_from_slice(&genesis_challenge);

        Bytes32::new(Keccak256::digest(&to_hash).into())
    }

    pub fn prefix_and_domain_separator(
        genesis_challenge: Bytes32,
    ) -> Eip712PrefixAndDomainSeparator {
        let mut pads = [0u8; 34];
        pads[0] = 0x19;
        pads[1] = 0x01;
        pads[2..].copy_from_slice(&Self::domain_separator(genesis_challenge));
        pads.into()
    }

    pub fn type_hash() -> Bytes32 {
        Bytes32::new(
            Keccak256::digest(b"ChiaCoinSpend(bytes32 coin_id,bytes32 delegated_puzzle_hash)")
                .into(),
        )
    }

    pub fn hash_to_sign(&self, coin_id: Bytes32, delegated_puzzle_hash: Bytes32) -> Bytes32 {
        /*
        bytes32 messageHash = keccak256(abi.encode(
            typeHash,
            coin_id,
            delegated_puzzle_hash
        ));
        */
        let mut to_hash = Vec::new();
        to_hash.extend_from_slice(&P2Eip712MessageLayer::type_hash());
        to_hash.extend_from_slice(&coin_id);
        to_hash.extend_from_slice(&delegated_puzzle_hash);

        let message_hash = Keccak256::digest(&to_hash);

        let mut to_hash = Vec::new();
        to_hash.extend_from_slice(&self.prefix_and_domain_separator);
        to_hash.extend_from_slice(&message_hash);

        Bytes32::new(Keccak256::digest(&to_hash).into())
    }
}

impl Layer for P2Eip712MessageLayer {
    type Solution = P2Eip712MessageSolution<NodePtr, NodePtr>;

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let curried = CurriedProgram {
            program: ctx.p2_eip712_message_puzzle()?,
            args: P2Eip712MessageArgs {
                prefix_and_domain_separator: self.prefix_and_domain_separator,
                type_hash: P2Eip712MessageLayer::type_hash(),
                pubkey: self.pubkey,
            },
        };
        ctx.alloc(&curried)
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        ctx.alloc(&solution)
    }

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
            pubkey: args.pubkey,
        }))
    }

    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        Ok(P2Eip712MessageSolution::from_clvm(allocator, solution)?)
    }
}

#[cfg(test)]
mod tests {
    use crate::assert_puzzle_hash;

    use super::*;
    use chia_consensus::consensus_constants::TEST_CONSTANTS;
    use chia_protocol::Bytes;
    use chia_sdk_test::Simulator;
    use chia_sdk_types::Conditions;
    use clvm_traits::clvm_quote;
    use clvm_utils::ToTreeHash;
    use clvmr::chia_dialect::ENABLE_KECCAK_OPS_OUTSIDE_GUARD;
    use clvmr::reduction::Reduction;
    use clvmr::serde::node_from_bytes;
    use k256::ecdsa::signature::hazmat::PrehashVerifier;
    use k256::ecdsa::{Signature as K1Signature, SigningKey, VerifyingKey as K1VerifyingKey};
    use k256::elliptic_curve::rand_core::OsRng;
    use rstest::rstest;

    #[test]
    fn test_puzzle_hash() -> anyhow::Result<()> {
        assert_puzzle_hash!(P2_EIP712_MESSAGE_PUZZLE => P2_EIP712_MESSAGE_PUZZLE_HASH);
        Ok(())
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
    fn test_domain_separator() {
        assert_eq!(
            P2Eip712MessageLayer::domain_separator(TEST_CONSTANTS.genesis_challenge),
            Bytes32::new(hex!(
                "acfd7ee1b3beb56b11d29d9e48debee9edf2f457d1dbdc19b63e58a6884501af"
            ))
        );
    }

    #[test]
    fn test_softfork_cost() -> anyhow::Result<()> {
        let ctx = &mut SpendContext::new();
        // code running inside softfork
        // run -d '(mod (PREFIX_AND_DOMAIN_SEPARATOR TYPE_HASH my_id delegated_puzzle_hash signed_hash) (if (= (keccak256 PREFIX_AND_DOMAIN_SEPARATOR (keccak256 TYPE_HASH my_id delegated_puzzle_hash)) signed_hash) () (x)))'
        let puzzle_bytes =
            hex!("ff02ffff03ffff09ffff3eff02ffff3eff05ff0bff178080ff2f80ff80ffff01ff088080ff0180");

        let puzzle_ptr = node_from_bytes(&mut ctx.allocator, puzzle_bytes.as_slice())?;
        let solution_ptr = vec![
            Bytes::new(
                hex!("1901098ccd7d09a29365582c3f7590712bc2c2eb8503586f8a4c628c61c73ffbe4aa")
                    .to_vec(),
            ), // PREFIX_AND_DOMAIN_SEPARATOR (different than those used on testnet11/mainnet)
            Bytes::new(
                hex!("72930978f119c79f9de7a13bd50c9b3261132d7b4819bdf0d3ca4d4c37ade070").to_vec(),
            ), // TYPE_HASH
            Bytes::new(
                hex!("5c777c45fd52a17a54e420742cadc56172847d9a106ff0ff8af38ef757d84829").to_vec(),
            ), // my_id
            Bytes::new(
                hex!("d842dfa1453a130a8be66bc32708a2d1884662d7daaa4aae530be3259fa6712f").to_vec(),
            ), // delegated_puzzle_hash
            Bytes::new(
                hex!("9f61fdf6077c3eeb96eaa4dd450b11ba3ae17746a2c304388218137972c7ba4c").to_vec(),
            ), // signed_hash
        ]
        .to_clvm(&mut ctx.allocator)?;

        let Reduction(cost, _) = clvmr::run_program(
            &mut ctx.allocator,
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
        let signing_key = SigningKey::random(&mut OsRng);

        // actual test
        let ctx = &mut SpendContext::new();
        let mut sim = Simulator::new();

        let pubkey = signing_key.verifying_key().to_sec1_bytes().to_vec();
        let layer = P2Eip712MessageLayer::from_genesis_challenge(
            pubkey.try_into().unwrap(),
            TEST_CONSTANTS.genesis_challenge,
        );
        let coin_puzzle_reveal = layer.construct_puzzle(ctx)?;
        let coin_puzzle_hash = ctx.tree_hash(coin_puzzle_reveal);

        let coin = sim.new_coin(coin_puzzle_hash.into(), 1337);

        let delegated_puzzle_ptr =
            clvm_quote!(Conditions::new().reserve_fee(1337)).to_clvm(&mut ctx.allocator)?;
        let delegated_solution_ptr = ctx.allocator.nil();

        let hash_to_sign: Bytes32 = if correct_signed_hash {
            layer.hash_to_sign(coin.coin_id(), ctx.tree_hash(delegated_puzzle_ptr).into())
        } else {
            layer
                .hash_to_sign(coin.coin_id(), ctx.tree_hash(delegated_puzzle_ptr).into())
                .tree_hash()
                .into()
        };

        let signature_og: K1Signature = signing_key
            .sign_prehash_recoverable(&hash_to_sign.to_vec())?
            .0;
        let signature: EthSignatureBytes = signature_og.to_vec().try_into().unwrap();

        let coin_spend = layer.construct_coin_spend(
            ctx,
            coin,
            P2Eip712MessageSolution {
                my_id: coin.coin_id(),
                signed_hash: hash_to_sign,
                signature,
                delegated_puzzle: delegated_puzzle_ptr,
                delegated_solution: delegated_solution_ptr,
            },
        )?;

        ctx.insert(coin_spend);

        let verifier =
            K1VerifyingKey::from_sec1_bytes(&signing_key.verifying_key().to_sec1_bytes())?;
        assert_eq!(verifier, *signing_key.verifying_key());
        let msg = hash_to_sign.to_vec();
        let sig = K1Signature::from_slice(&signature)?;
        assert_eq!(sig, K1Signature::from_slice(&signature_og.to_vec())?);
        let result = verifier.verify_prehash(msg.as_ref(), &sig);
        assert!(result.is_ok());

        if correct_signed_hash {
            sim.spend_coins(ctx.take(), &[])?;
        } else {
            assert_eq!(
                sim.spend_coins(ctx.take(), &[])
                    .err()
                    .unwrap()
                    .to_string()
                    .split(": ")
                    .last()
                    .unwrap(),
                "clvm raise"
            );
        }

        Ok(())
    }
}
