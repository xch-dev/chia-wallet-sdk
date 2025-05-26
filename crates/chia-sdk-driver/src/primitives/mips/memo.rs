use chia_bls::PublicKey;
use chia_protocol::Bytes32;
use chia_sdk_types::{
    puzzles::{
        BlsMember, FixedPuzzleMember, PasskeyMember, PasskeyMemberPuzzleAssert, Secp256k1Member,
        Secp256k1MemberPuzzleAssert, Secp256r1Member, Secp256r1MemberPuzzleAssert, SingletonMember,
    },
    Mod,
};
use chia_secp::{K1PublicKey, R1PublicKey};
use clvm_traits::{apply_constants, FromClvm, ToClvm};
use clvmr::NodePtr;

#[derive(ToClvm, FromClvm)]
#[apply_constants]
#[derive(Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct MipsMemo<T = NodePtr> {
    #[clvm(constant = "CHIP-0043".to_string())]
    pub namespace: String,
    pub inner_puzzle: InnerPuzzleMemo<T>,
}

impl MipsMemo<NodePtr> {
    pub fn new(inner_puzzle: InnerPuzzleMemo) -> Self {
        Self { inner_puzzle }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct InnerPuzzleMemo<T = NodePtr> {
    pub nonce: usize,
    pub restrictions: Vec<RestrictionMemo<T>>,
    #[clvm(rest)]
    pub kind: MemoKind<T>,
}

impl InnerPuzzleMemo<NodePtr> {
    pub fn new(nonce: usize, restrictions: Vec<RestrictionMemo>, kind: MemoKind) -> Self {
        Self {
            nonce,
            restrictions,
            kind,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct RestrictionMemo<T = NodePtr> {
    pub member_condition_validator: bool,
    pub puzzle_hash: Bytes32,
    pub memo: T,
}

impl RestrictionMemo<NodePtr> {
    pub fn new(member_condition_validator: bool, puzzle_hash: Bytes32, memo: NodePtr) -> Self {
        Self {
            member_condition_validator,
            puzzle_hash,
            memo,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub enum MemoKind<T = NodePtr> {
    Member(#[clvm(rest)] MemberMemo<T>),
    MofN {
        required: usize,
        items: Vec<InnerPuzzleMemo<T>>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct MemberMemo<T = NodePtr> {
    pub puzzle_hash: Bytes32,
    pub memo: T,
}

impl MemberMemo<NodePtr> {
    pub fn new(puzzle_hash: Bytes32, memo: NodePtr) -> Self {
        Self { puzzle_hash, memo }
    }

    pub fn k1(public_key: K1PublicKey, fast_forward: bool) -> Self {
        Self::new(
            if fast_forward {
                Secp256k1MemberPuzzleAssert::new(public_key).curry_tree_hash()
            } else {
                Secp256k1Member::new(public_key).curry_tree_hash()
            }
            .into(),
            NodePtr::NIL,
        )
    }

    pub fn r1(public_key: R1PublicKey, fast_forward: bool) -> Self {
        Self::new(
            if fast_forward {
                Secp256r1MemberPuzzleAssert::new(public_key).curry_tree_hash()
            } else {
                Secp256r1Member::new(public_key).curry_tree_hash()
            }
            .into(),
            NodePtr::NIL,
        )
    }

    pub fn bls(public_key: PublicKey) -> Self {
        Self::new(
            BlsMember::new(public_key).curry_tree_hash().into(),
            NodePtr::NIL,
        )
    }

    pub fn passkey(public_key: R1PublicKey, fast_forward: bool) -> Self {
        Self::new(
            if fast_forward {
                PasskeyMemberPuzzleAssert::new(public_key).curry_tree_hash()
            } else {
                PasskeyMember::new(public_key).curry_tree_hash()
            }
            .into(),
            NodePtr::NIL,
        )
    }

    pub fn singleton(launcher_id: Bytes32) -> Self {
        Self::new(
            SingletonMember::new(launcher_id).curry_tree_hash().into(),
            NodePtr::NIL,
        )
    }

    pub fn fixed_puzzle(puzzle_hash: Bytes32) -> Self {
        Self::new(
            FixedPuzzleMember::new(puzzle_hash).curry_tree_hash().into(),
            NodePtr::NIL,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct MofNMemo<T = NodePtr> {
    pub required: usize,
    pub items: Vec<InnerPuzzleMemo<T>>,
}

impl MofNMemo<NodePtr> {
    pub fn new(required: usize, items: Vec<InnerPuzzleMemo>) -> Self {
        Self { required, items }
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use clvmr::Allocator;

    use super::*;

    #[test]
    fn test_mips_memo() -> Result<()> {
        let mut allocator = Allocator::new();

        let memo = MipsMemo::new(InnerPuzzleMemo::new(
            0,
            vec![],
            MemoKind::member(Bytes32::default(), NodePtr::NIL),
        ));

        Ok(())
    }
}
