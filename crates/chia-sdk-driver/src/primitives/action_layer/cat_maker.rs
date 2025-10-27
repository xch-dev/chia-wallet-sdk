use chia_protocol::Bytes32;
use chia_sdk_types::{
    Mod,
    puzzles::{
        DEFAULT_CAT_MAKER_PUZZLE_HASH, DefaultCatMakerArgs, REVOCABLE_CAT_MAKER_PUZZLE_HASH,
        RevocableCatMakerArgs, XCH_CAT_MAKER_PUZZLE_HASH, XchCatMaker,
    },
};
use clvm_traits::{FromClvm, ToClvm, clvm_tuple};
use clvm_utils::TreeHash;
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Puzzle, SpendContext};

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

    pub fn parse_puzzle(
        allocator: &Allocator,
        puzzle: Puzzle,
    ) -> Result<Option<Self>, DriverError> {
        let puzzle_mod_hash = puzzle.mod_hash();

        if puzzle_mod_hash == XCH_CAT_MAKER_PUZZLE_HASH {
            return Ok(Some(Self::Xch));
        }

        if let Some(curried_puzzle) = puzzle.as_curried() {
            if puzzle_mod_hash == DEFAULT_CAT_MAKER_PUZZLE_HASH {
                let args = DefaultCatMakerArgs::from_clvm(allocator, curried_puzzle.args)?;

                return Ok(Some(Self::Default {
                    tail_hash_hash: args.tail_hash_hash.into(),
                }));
            } else if puzzle_mod_hash == REVOCABLE_CAT_MAKER_PUZZLE_HASH {
                let args = RevocableCatMakerArgs::from_clvm(allocator, curried_puzzle.args)?;

                return Ok(Some(Self::Revocable {
                    tail_hash_hash: args.tail_hash_hash.into(),
                    hidden_puzzle_hash_hash: args.mod_struct.hidden_puzzle_hash_hash.into(),
                }));
            }
        }

        Ok(None)
    }
}
