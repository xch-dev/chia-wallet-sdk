use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzles::{SECP256R1_MEMBER, SECP256R1_MEMBER_HASH};
use chia_secp::{R1PublicKey, R1Signature};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;

use crate::Mod;

/// Curried arguments for the MIPS secp256r1 member puzzle, which approves a delegated puzzle hash
/// with a secp256r1 signature.
///
/// The signature must be on the SHA-256 hash of the delegated puzzle hash followed by the coin id,
/// and the puzzle outputs `ASSERT_MY_COIN_ID` so the signature is bound to the coin being spent.
///
/// Like all MIPS members, it receives the delegated puzzle hash as the first value of its solution
/// from the layer above, followed by the member's own solution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct R1Member {
    /// The secp256r1 public key that must sign.
    pub public_key: R1PublicKey,
}

impl R1Member {
    /// Creates a new [`R1Member`] from the public key.
    pub fn new(public_key: R1PublicKey) -> Self {
        Self { public_key }
    }
}

impl Mod for R1Member {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&SECP256R1_MEMBER)
    }

    fn mod_hash() -> TreeHash {
        SECP256R1_MEMBER_HASH.into()
    }
}

/// The member solution for [`R1Member`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct R1MemberSolution {
    /// The coin id of the coin being spent.
    pub coin_id: Bytes32,
    /// The signature on the hash of the delegated puzzle hash and coin id.
    pub signature: R1Signature,
}

impl R1MemberSolution {
    /// Creates a new [`R1MemberSolution`] from the coin id and signature.
    pub fn new(coin_id: Bytes32, signature: R1Signature) -> Self {
        Self { coin_id, signature }
    }
}
