use chia_protocol::{Bytes32, Coin};
use chia_puzzles::{
    singleton::{SingletonArgs, SINGLETON_LAUNCHER_PUZZLE_HASH, SINGLETON_TOP_LAYER_PUZZLE_HASH},
    LineageProof,
};
use clvm_traits::FromClvm;
use clvmr::{Allocator, NodePtr};

use crate::{ParseError, Puzzle};

#[derive(Debug, Clone, Copy)]
pub struct SingletonPuzzle {
    pub launcher_id: Bytes32,
    pub inner_puzzle: Puzzle,
}

impl SingletonPuzzle {
    pub fn parse(
        allocator: &Allocator,
        puzzle: &Puzzle,
    ) -> Result<Option<SingletonPuzzle>, ParseError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != SINGLETON_TOP_LAYER_PUZZLE_HASH {
            return Ok(None);
        }

        let args = SingletonArgs::<NodePtr>::from_clvm(allocator, puzzle.args)?;

        if args.singleton_struct.mod_hash != SINGLETON_TOP_LAYER_PUZZLE_HASH.into()
            || args.singleton_struct.launcher_puzzle_hash != SINGLETON_LAUNCHER_PUZZLE_HASH.into()
        {
            return Err(ParseError::InvalidSingletonStruct);
        }

        Ok(Some(SingletonPuzzle {
            launcher_id: args.singleton_struct.launcher_id,
            inner_puzzle: Puzzle::parse(allocator, args.inner_puzzle),
        }))
    }

    pub fn lineage_proof(&self, parent_coin: Coin) -> LineageProof {
        LineageProof {
            parent_parent_coin_id: parent_coin.parent_coin_info,
            parent_inner_puzzle_hash: self.inner_puzzle.curried_puzzle_hash().into(),
            parent_amount: parent_coin.amount,
        }
    }
}
