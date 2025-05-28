use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;

use crate::Mod;

use super::{
    BlsMember, BlsTaprootMember, FixedPuzzleMember, K1Member, K1MemberPuzzleAssert, PasskeyMember,
    PasskeyMemberPuzzleAssert, R1Member, R1MemberPuzzleAssert, SingletonMember,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(transparent)]
pub enum Member {
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

impl Member {
    pub fn curry_tree_hash(&self) -> TreeHash {
        match self {
            Self::K1(member) => member.curry_tree_hash(),
            Self::K1PuzzleAssert(member) => member.curry_tree_hash(),
            Self::R1(member) => member.curry_tree_hash(),
            Self::R1PuzzleAssert(member) => member.curry_tree_hash(),
            Self::Bls(member) => member.curry_tree_hash(),
            Self::BlsTaproot(member) => member.curry_tree_hash(),
            Self::Passkey(member) => member.curry_tree_hash(),
            Self::PasskeyPuzzleAssert(member) => member.curry_tree_hash(),
            Self::Singleton(member) => member.curry_tree_hash(),
            Self::FixedPuzzle(member) => member.curry_tree_hash(),
        }
    }
}
