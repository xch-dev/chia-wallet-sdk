use std::borrow::Cow;

use chia_protocol::{Bytes, Bytes32};
use chia_puzzles::{PASSKEY_MEMBER_PUZZLE_ASSERT, PASSKEY_MEMBER_PUZZLE_ASSERT_HASH};
use chia_secp::{R1PublicKey, R1Signature};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;

use crate::Mod;

/// Curried arguments for the MIPS passkey member puzzle, which approves a delegated puzzle hash
/// with a `WebAuthn` (passkey) signature, such as from a security key or a phone.
///
/// The `WebAuthn` challenge must be the unpadded base64url encoding of the SHA-256 hash of the
/// delegated puzzle hash followed by the coin's puzzle hash. The puzzle checks that
/// `"challenge":"<challenge>"` appears in the client data JSON at the given index, verifies the
/// secp256r1 signature on the SHA-256 hash of the authenticator data followed by the SHA-256 hash
/// of the client data JSON, and outputs `ASSERT_MY_PUZZLE_HASH`. The signature can be reused for
/// any coin with the same puzzle hash.
///
/// Like all MIPS members, it receives the delegated puzzle hash as the first value of its solution
/// from the layer above, followed by the member's own solution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct PasskeyMemberPuzzleAssert {
    /// The secp256r1 public key of the passkey.
    pub public_key: R1PublicKey,
}

impl PasskeyMemberPuzzleAssert {
    /// Creates a new [`PasskeyMemberPuzzleAssert`] from the passkey's public key.
    pub fn new(public_key: R1PublicKey) -> Self {
        Self { public_key }
    }
}

impl Mod for PasskeyMemberPuzzleAssert {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&PASSKEY_MEMBER_PUZZLE_ASSERT)
    }

    fn mod_hash() -> TreeHash {
        PASSKEY_MEMBER_PUZZLE_ASSERT_HASH.into()
    }
}

/// The member solution for [`PasskeyMemberPuzzleAssert`].
#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct PasskeyMemberPuzzleAssertSolution {
    /// The authenticator data returned by the `WebAuthn` API.
    pub authenticator_data: Bytes,
    /// The client data JSON returned by the `WebAuthn` API.
    pub client_data_json: Bytes,
    /// The byte index in the client data JSON where `"challenge":` starts.
    pub challenge_index: usize,
    /// The signature returned by the `WebAuthn` API.
    pub signature: R1Signature,
    /// The puzzle hash of the coin being spent.
    pub puzzle_hash: Bytes32,
}
