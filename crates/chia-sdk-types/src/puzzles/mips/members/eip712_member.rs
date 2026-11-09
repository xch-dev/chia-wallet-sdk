use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_secp::{K1PublicKey, K1Signature};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{Mod, puzzles::Eip712PrefixAndDomainSeparator};

/// A CHIP-0043 MIPS member puzzle controlled by an Ethereum-style secp256k1
/// key that signs the canonical CHIP-0037 EIP-712 digest of
/// `ChiaCoinSpend(bytes32 coin_id, bytes32 delegated_puzzle_hash)`.
///
/// Composes with the existing `m_of_n`, `n_of_n`, and `one_of_n` quorum
/// puzzles, restrictions, and delegated-puzzle wrappers in the same way as
/// [`K1Member`](super::K1Member) and [`R1Member`](super::R1Member).
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct Eip712Member {
    pub prefix_and_domain_separator: Eip712PrefixAndDomainSeparator,
    pub type_hash: Bytes32,
    pub public_key: K1PublicKey,
}

impl Eip712Member {
    pub fn new(
        prefix_and_domain_separator: Eip712PrefixAndDomainSeparator,
        type_hash: Bytes32,
        public_key: K1PublicKey,
    ) -> Self {
        Self {
            prefix_and_domain_separator,
            type_hash,
            public_key,
        }
    }
}

impl Mod for Eip712Member {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&EIP712_MEMBER)
    }

    fn mod_hash() -> TreeHash {
        EIP712_MEMBER_HASH
    }
}

/// Member-specific solution carried by an [`Eip712Member`] spend.
///
/// `Delegated_Puzzle_Hash` is supplied by the parent M-of-N layer as a truth
/// rather than via this struct, matching the convention used by every other
/// MIPS member type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct Eip712MemberSolution {
    pub my_id: Bytes32,
    pub signed_hash: Bytes32,
    pub signature: K1Signature,
}

impl Eip712MemberSolution {
    pub fn new(my_id: Bytes32, signed_hash: Bytes32, signature: K1Signature) -> Self {
        Self {
            my_id,
            signed_hash,
            signature,
        }
    }
}

/// CLVM bytecode for the CHIP-0043 `eip712_member` puzzle.
///
/// Source: <https://github.com/MattHintz/solslot-protocol/blob/108ac023d836cf0198775eebd377880d0f8d6aef/solslot_puzzles/eip712_member_v2.clsp>
///
/// Adapted from the CHIP-0037 `p2_eip712_message` puzzle in Yakuhito's Hermes:
/// <https://github.com/Yakuhito/hermes/blob/0f4779721ceb395129c3577eb3b4e0b3da708cfc/clsp/p2_eip712_message.clsp>
pub const EIP712_MEMBER: [u8; 171] = hex!(
    "
    ff02ffff01ff02ffff03ffff22ffff20ffff8413d61f00ff17ff81bfff82017f
    8080ffff20ffff24ffff01820ab9ffff0101ffff01ff02ffff03ffff09ffff3e
    ff02ffff3eff05ff0bff178080ff2f80ff80ffff01ff088080ff0180ffff04ff
    05ffff04ff0bffff04ff5fffff04ff2fffff04ff81bfff808080808080808080
    ffff01ff04ffff04ff02ffff04ff5fff808080ff8080ffff01ff088080ff0180
    ffff04ffff0146ff018080
    "
);

pub const EIP712_MEMBER_HASH: TreeHash = TreeHash::new(hex!(
    "aaf5d0676da9c06b02ae94acc7f226c6b583205d42f5b350bec43df7fcd09ea7"
));

#[cfg(test)]
mod tests {
    use super::*;

    use chia_secp::K1SecretKey;
    use clvm_traits::clvm_list;
    use clvm_utils::CurriedProgram;
    use clvmr::{Allocator, serde::node_from_bytes, serde::node_to_bytes};
    use rand::{RngExt, SeedableRng};
    use rand_chacha::ChaCha8Rng;
    use rstest::rstest;

    use crate::{
        assert_puzzle_hash,
        puzzles::{eip712_hash_to_sign, eip712_prefix_and_domain_separator, eip712_type_hash},
        run_puzzle,
    };

    const ASSERT_MY_COIN_ID: u8 = 70;

    #[test]
    fn test_puzzle_hash() -> anyhow::Result<()> {
        assert_puzzle_hash!(EIP712_MEMBER => EIP712_MEMBER_HASH);

        Ok(())
    }

    /// * A valid EIP-712 signature returns `((ASSERT_MY_COIN_ID my_id))`.
    /// * Signing a `signed_hash` that doesn't match this spend passes
    ///   `secp256k1_verify` but fails the keccak256 reconstruction.
    /// * Tampering with the signature makes `secp256k1_verify` raise.
    #[rstest]
    #[case::valid(true, true)]
    #[case::wrong_signed_hash(false, true)]
    #[case::tampered_signature(true, false)]
    fn test_eip712_member_runs(
        #[case] signed_hash_matches: bool,
        #[case] signature_matches: bool,
    ) -> anyhow::Result<()> {
        let mut rng = ChaCha8Rng::seed_from_u64(0x1234_5678);
        let sk = K1SecretKey::from_bytes(&rng.random())?;
        let pk = sk.public_key();

        let prefix_and_domain = eip712_prefix_and_domain_separator(Bytes32::new(rng.random()));
        let coin_id = Bytes32::new(rng.random());
        let delegated_puzzle_hash = Bytes32::new(rng.random());

        let signed_hash = if signed_hash_matches {
            eip712_hash_to_sign(&prefix_and_domain, coin_id, delegated_puzzle_hash)
        } else {
            Bytes32::new(rng.random())
        };

        let mut signature = sk.sign_prehashed(&signed_hash.to_bytes())?;

        if !signature_matches {
            let mut bytes = signature.to_bytes();
            bytes[0] ^= 0x01;
            signature = K1Signature::from_bytes(&bytes)?;
        }

        let mut allocator = Allocator::new();
        let mod_ptr = node_from_bytes(&mut allocator, &EIP712_MEMBER)?;
        let puzzle = CurriedProgram {
            program: mod_ptr,
            args: Eip712Member::new(prefix_and_domain, eip712_type_hash(), pk),
        }
        .to_clvm(&mut allocator)?;

        // The MIPS parent passes `Delegated_Puzzle_Hash` before the member's own solution.
        let solution = clvm_list!(delegated_puzzle_hash, coin_id, signed_hash, signature)
            .to_clvm(&mut allocator)?;

        let result = run_puzzle(&mut allocator, puzzle, solution);

        if signed_hash_matches && signature_matches {
            let expected =
                clvm_list!(clvm_list!(ASSERT_MY_COIN_ID, coin_id)).to_clvm(&mut allocator)?;
            assert_eq!(
                node_to_bytes(&allocator, result?)?,
                node_to_bytes(&allocator, expected)?
            );
        } else {
            assert!(result.is_err());
        }

        Ok(())
    }
}
