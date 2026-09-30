use chia_sdk_types::puzzles::{
    BlsMemberPuzzleAssert, BlsTaprootMemberPuzzleAssert, SingletonMemberWithMode,
};
use clvmr::NodePtr;

use super::{
    BlsMember, BlsTaprootMember, FixedPuzzleMember, K1Member, K1MemberPuzzleAssert, PasskeyMember,
    PasskeyMemberPuzzleAssert, R1Member, R1MemberPuzzleAssert, SingletonMember,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParsedMember {
    K1(K1Member),
    K1PuzzleAssert(K1MemberPuzzleAssert),
    R1(R1Member),
    R1PuzzleAssert(R1MemberPuzzleAssert),
    Bls(BlsMember),
    BlsPuzzleAssert(BlsMemberPuzzleAssert),
    BlsTaproot(BlsTaprootMember),
    BlsTaprootPuzzleAssert(BlsTaprootMemberPuzzleAssert),
    Passkey(PasskeyMember),
    PasskeyPuzzleAssert(PasskeyMemberPuzzleAssert),
    Singleton(SingletonMember),
    SingletonWithMode(SingletonMemberWithMode),
    FixedPuzzle(FixedPuzzleMember),
    /// A member whose memo is the full puzzle reveal.
    Custom(NodePtr),
}
