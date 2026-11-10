use chia_sdk_types::puzzles::{
    BlsMemberPuzzleAssert, BlsTaprootMemberPuzzleAssert, SingletonMemberWithMode,
};
use clvmr::NodePtr;

use super::{
    BlsMember, BlsTaprootMember, FixedPuzzleMember, K1Member, K1MemberPuzzleAssert, PasskeyMember,
    PasskeyMemberPuzzleAssert, R1Member, R1MemberPuzzleAssert, SingletonMember,
};

/// A MIPS member identified by [`MemberMemo::parse`](crate::MemberMemo::parse).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParsedMember {
    /// A secp256k1 member.
    K1(K1Member),
    /// A secp256k1 member that signs with the puzzle hash instead of the coin id.
    K1PuzzleAssert(K1MemberPuzzleAssert),
    /// A secp256r1 member.
    R1(R1Member),
    /// A secp256r1 member that signs with the puzzle hash instead of the coin id.
    R1PuzzleAssert(R1MemberPuzzleAssert),
    /// A BLS member.
    Bls(BlsMember),
    /// A BLS member that signs with the puzzle hash instead of the coin id.
    BlsPuzzleAssert(BlsMemberPuzzleAssert),
    /// A BLS taproot member.
    BlsTaproot(BlsTaprootMember),
    /// A BLS taproot member that signs with the puzzle hash instead of the coin id.
    BlsTaprootPuzzleAssert(BlsTaprootMemberPuzzleAssert),
    /// A passkey member.
    Passkey(PasskeyMember),
    /// A passkey member that signs with the puzzle hash instead of the coin id.
    PasskeyPuzzleAssert(PasskeyMemberPuzzleAssert),
    /// A member that is authorized by a message from a singleton.
    Singleton(SingletonMember),
    /// A member that is authorized by a message from a singleton, with a custom message mode.
    SingletonWithMode(SingletonMemberWithMode),
    /// A member that only allows a delegated puzzle with a fixed hash.
    FixedPuzzle(FixedPuzzleMember),
    /// A member whose memo is the full puzzle reveal.
    Custom(NodePtr),
}
