use chia_protocol::Bytes32;
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
    Member {
        puzzle_hash: Bytes32,
        memo: T,
    },
    MofN {
        required: usize,
        items: Vec<InnerPuzzleMemo<T>>,
    },
}

impl MemoKind<NodePtr> {
    pub fn member(puzzle_hash: Bytes32, memo: NodePtr) -> Self {
        Self::Member { puzzle_hash, memo }
    }

    pub fn mofn(required: usize, items: Vec<InnerPuzzleMemo>) -> Self {
        Self::MofN { required, items }
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
