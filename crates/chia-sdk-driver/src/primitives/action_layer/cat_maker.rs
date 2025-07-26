use chia_protocol::Bytes32;
use chia_sdk_types::{
    puzzles::{DefaultCatMakerArgs, RevocableCatMakerArgs, XchCatMaker, XCH_CAT_MAKER_PUZZLE_HASH},
    Mod,
};
use clvm_traits::{clvm_tuple, ToClvm};
use clvm_utils::TreeHash;
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, SpendContext};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatMaker {
    Default {
        tail_hash_hash: TreeHash,
    },
    Revocable {
        tail_hash_hash: TreeHash,
        hidden_puzzle_hash_hash: TreeHash,
    },
    Xch,
}

impl CatMaker {
    pub fn curry_tree_hash(&self) -> TreeHash {
        match self {
            CatMaker::Default { tail_hash_hash } => {
                DefaultCatMakerArgs::new((*tail_hash_hash).into()).curry_tree_hash()
            }
            CatMaker::Revocable {
                tail_hash_hash,
                hidden_puzzle_hash_hash,
            } => RevocableCatMakerArgs::new(*tail_hash_hash, *hidden_puzzle_hash_hash)
                .curry_tree_hash(),
            CatMaker::Xch => XCH_CAT_MAKER_PUZZLE_HASH,
        }
    }

    pub fn get_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        match self {
            CatMaker::Default { tail_hash_hash } => {
                ctx.curry(DefaultCatMakerArgs::new((*tail_hash_hash).into()))
            }
            CatMaker::Revocable {
                tail_hash_hash,
                hidden_puzzle_hash_hash,
            } => ctx.curry(RevocableCatMakerArgs::new(
                *tail_hash_hash,
                *hidden_puzzle_hash_hash,
            )),
            CatMaker::Xch => ctx.alloc_mod::<XchCatMaker>(),
        }
    }

    pub fn run<S>(
        &self,
        ctx: &mut SpendContext,
        inner_puzzle_hash: Bytes32,
        solution_rest: S,
    ) -> Result<Bytes32, DriverError>
    where
        S: ToClvm<Allocator>,
    {
        let solution = clvm_tuple!(inner_puzzle_hash, solution_rest);
        let solution = ctx.alloc(&solution)?;

        let puzzle = self.get_puzzle(ctx)?;
        let result = ctx.run(puzzle, solution)?;

        ctx.extract(result)
    }
}
