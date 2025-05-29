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
    BlsTaproot(BlsTaprootMember),
    Passkey(PasskeyMember),
    PasskeyPuzzleAssert(PasskeyMemberPuzzleAssert),
    Singleton(SingletonMember),
    FixedPuzzle(FixedPuzzleMember),
}
