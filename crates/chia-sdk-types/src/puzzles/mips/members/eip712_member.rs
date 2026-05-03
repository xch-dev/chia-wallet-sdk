use chia_protocol::Bytes32;
use chia_secp::{K1PublicKey, K1Signature};
use clvm_traits::{FromClvm, ToClvm};

use crate::{Compilation, compile_chialisp, puzzles::Eip712PrefixAndDomainSeparator};

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

compile_chialisp!(Eip712Member = EIP712_MEMBER, "eip712_member.clsp");

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

#[cfg(test)]
mod tests {
    use super::*;

    use chia_secp::K1SecretKey;
    use clvm_traits::clvm_list;
    use clvm_utils::CurriedProgram;
    use clvmr::{Allocator, serde::node_from_bytes, serde::node_to_bytes};
    use hex_literal::hex;
    use rand::{Rng, SeedableRng};
    use rand_chacha::ChaCha8Rng;
    use rstest::rstest;
    use sha3::{Digest, Keccak256};

    use crate::{Mod, run_puzzle};

    /// ASSERT_MY_COIN_ID condition opcode (matches the inline value in
    /// `eip712_member.clsp`).
    const ASSERT_MY_COIN_ID: u8 = 73;

    /// Compute the canonical CHIP-0037 EIP-712 type hash for
    /// `ChiaCoinSpend(bytes32 coin_id, bytes32 delegated_puzzle_hash)`.
    fn type_hash() -> Bytes32 {
        Bytes32::new(
            Keccak256::digest(b"ChiaCoinSpend(bytes32 coin_id,bytes32 delegated_puzzle_hash)")
                .into(),
        )
    }

    /// Compute `prefix (0x1901) || domain_separator(genesis_challenge)`.
    fn prefix_and_domain_separator(genesis_challenge: Bytes32) -> Eip712PrefixAndDomainSeparator {
        let dom_type_hash =
            Keccak256::digest(b"EIP712Domain(string name,string version,bytes32 salt)");
        let mut to_hash = Vec::with_capacity(128);
        to_hash.extend_from_slice(&dom_type_hash);
        to_hash.extend_from_slice(&Keccak256::digest(b"Chia Coin Spend"));
        to_hash.extend_from_slice(&Keccak256::digest(b"1"));
        to_hash.extend_from_slice(&genesis_challenge);
        let domain_separator = Keccak256::digest(&to_hash);

        let mut bytes = [0u8; 34];
        bytes[0] = 0x19;
        bytes[1] = 0x01;
        bytes[2..].copy_from_slice(&domain_separator);
        bytes.into()
    }

    /// Compute the EIP-712 digest the off-chain wallet must sign:
    /// `keccak256(prefix_and_domain || keccak256(type_hash || coin_id ||
    /// delegated_puzzle_hash))`.
    fn hash_to_sign(
        prefix_and_domain: &Eip712PrefixAndDomainSeparator,
        type_hash: Bytes32,
        coin_id: Bytes32,
        delegated_puzzle_hash: Bytes32,
    ) -> Bytes32 {
        let mut to_hash = Vec::with_capacity(96);
        to_hash.extend_from_slice(&type_hash);
        to_hash.extend_from_slice(&coin_id);
        to_hash.extend_from_slice(&delegated_puzzle_hash);
        let inner = Keccak256::digest(&to_hash);

        let mut to_hash = Vec::with_capacity(34 + 32);
        to_hash.extend_from_slice(prefix_and_domain);
        to_hash.extend_from_slice(&inner);
        Bytes32::new(Keccak256::digest(&to_hash).into())
    }

    fn k1_pair(seed: u64) -> (K1SecretKey, K1PublicKey) {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let sk = K1SecretKey::from_bytes(&rng.random()).expect("K1SecretKey::from_bytes");
        let pk = sk.public_key();
        (sk, pk)
    }

    /// Smoke test: forcing `Eip712Member::mod_reveal()` triggers compilation
    /// of `eip712_member.clsp` via the in-process Chialisp compiler. If the
    /// source ever stops compiling, this test fails first.
    #[test]
    fn test_eip712_member_compiles() {
        let bytes = Eip712Member::mod_reveal();
        assert!(
            !bytes.is_empty(),
            "compiled Eip712Member bytecode is unexpectedly empty"
        );
    }

    /// `mod_hash` must be deterministic across calls (the underlying
    /// `LazyLock` caches the compilation, so any non-determinism would point
    /// at a `compile_chialisp` bug rather than at this puzzle).
    #[test]
    fn test_eip712_member_mod_hash_is_deterministic() {
        let a = Eip712Member::mod_hash();
        let b = Eip712Member::mod_hash();
        assert_eq!(a, b);
    }

    /// End-to-end happy path and negative path for the Chialisp logic:
    ///
    /// * The curried puzzle, given a valid EIP-712 signature, returns the
    ///   single-condition list `((ASSERT_MY_COIN_ID my_id))`.
    /// * Tampering with `signed_hash` causes the softfork-guarded keccak256
    ///   reconstruction to fail, raising `clvm raise`.
    /// * Tampering with the signature makes `secp256k1_verify` raise.
    ///
    /// Uses mainnet's genesis challenge as the EIP-712 domain salt so the
    /// fixture stays stable independently of `chia-consensus::TEST_CONSTANTS`.
    #[rstest]
    #[case::valid(true, true, true)]
    #[case::tampered_signed_hash(false, true, false)]
    #[case::tampered_signature(true, false, false)]
    fn test_eip712_member_runs(
        #[case] signed_hash_matches: bool,
        #[case] signature_matches: bool,
        #[case] expect_success: bool,
    ) -> anyhow::Result<()> {
        let (sk, pk) = k1_pair(0xC0DE_F00D);

        // Mainnet genesis challenge per chia-blockchain initial-config.yaml.
        let mainnet_genesis = Bytes32::new(hex!(
            "ccd5bb71183532bff220ba46c268991a3ff07eb358e8255a65c30a2dce0e5fbb"
        ));
        let prefix_and_domain = prefix_and_domain_separator(mainnet_genesis);
        let type_h = type_hash();

        // Pick fixed-seed test fixtures for `coin_id` and the
        // `delegated_puzzle_hash` the M-of-N parent would supply. They have no
        // intrinsic meaning here; they are just two distinct 32-byte values
        // that the puzzle binds the signature to.
        let mut rng = ChaCha8Rng::seed_from_u64(0x1234_5678);
        let coin_id_bytes: [u8; 32] = rng.random();
        let delegated_puzzle_hash_bytes: [u8; 32] = rng.random();
        let coin_id = Bytes32::new(coin_id_bytes);
        let delegated_puzzle_hash = Bytes32::new(delegated_puzzle_hash_bytes);

        let real_signed_hash =
            hash_to_sign(&prefix_and_domain, type_h, coin_id, delegated_puzzle_hash);

        // Always sign the *real* digest. If the test wants to simulate a bad
        // signature, we tamper with the `K1Signature` bytes after the fact so
        // `secp256k1_verify` raises; if it wants to simulate the wallet
        // committing to the wrong digest, we substitute a different
        // `signed_hash` in the solution so the keccak reconstruction raises.
        let real_signed_hash_bytes: [u8; 32] = real_signed_hash.into();
        let real_signature = sk.sign_prehashed(&real_signed_hash_bytes)?;

        let signed_hash_in_solution = if signed_hash_matches {
            real_signed_hash
        } else {
            // A 32-byte value the wallet would never produce for this spend.
            Bytes32::new([0xAA; 32])
        };

        let signature_in_solution = if signature_matches {
            real_signature
        } else {
            // Flip a byte to invalidate the ECDSA signature without changing
            // its length or canonical encoding.
            let mut tampered = real_signature.to_bytes();
            tampered[0] ^= 0x01;
            K1Signature::from_bytes(&tampered).expect("K1Signature::from_bytes")
        };

        let member = Eip712Member::new(prefix_and_domain, type_h, pk);

        let mut allocator = Allocator::new();
        let mod_ptr = node_from_bytes(&mut allocator, Eip712Member::mod_reveal().as_ref())?;
        let curried = CurriedProgram {
            program: mod_ptr,
            args: member,
        }
        .to_clvm(&mut allocator)?;

        // M-of-N supplies `delegated_puzzle_hash` as a positional arg before
        // the member's own solution. We mimic that here so the puzzle sees the
        // same env shape it would see at runtime.
        let solution = clvm_list!(
            delegated_puzzle_hash,
            coin_id,
            signed_hash_in_solution,
            signature_in_solution
        )
        .to_clvm(&mut allocator)?;

        let result = run_puzzle(&mut allocator, curried, solution);

        if expect_success {
            let conditions_ptr = result?;
            let expected =
                clvm_list!(clvm_list!(ASSERT_MY_COIN_ID, coin_id)).to_clvm(&mut allocator)?;
            assert_eq!(
                node_to_bytes(&allocator, conditions_ptr)?,
                node_to_bytes(&allocator, expected)?,
                "valid EIP-712 spend should return ((ASSERT_MY_COIN_ID my_id))"
            );
        } else {
            assert!(
                result.is_err(),
                "tampered EIP-712 spend should raise but returned conditions"
            );
        }

        Ok(())
    }
}
