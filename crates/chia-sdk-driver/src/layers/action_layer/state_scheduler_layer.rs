use chia_protocol::Bytes32;
use chia_puzzle_types::Memos;
use chia_puzzles::SINGLETON_TOP_LAYER_V1_1_HASH;
use chia_sdk_types::{
    puzzles::{StateSchedulerLayerArgs, StateSchedulerLayerSolution, STATE_SCHEDULER_PUZZLE_HASH},
    Condition, Conditions,
};
use clvm_traits::{clvm_quote, FromClvm};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, SpendContext};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateSchedulerLayer {
    pub receiver_singleton_struct_hash: Bytes32,
    pub new_state_hash: Bytes32,
    pub required_block_height: u32,
    pub new_puzzle_hash: Bytes32,
}

impl StateSchedulerLayer {
    pub fn new(
        receiver_singleton_struct_hash: Bytes32,
        new_state_hash: Bytes32,
        required_block_height: u32,
        new_puzzle_hash: Bytes32,
    ) -> Self {
        Self {
            receiver_singleton_struct_hash,
            new_state_hash,
            required_block_height,
            new_puzzle_hash,
        }
    }
}

impl Layer for StateSchedulerLayer {
    type Solution = StateSchedulerLayerSolution<()>;

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != STATE_SCHEDULER_PUZZLE_HASH {
            return Ok(None);
        }

        let args = StateSchedulerLayerArgs::<Bytes32, NodePtr>::from_clvm(allocator, puzzle.args)?;

        if args.singleton_mod_hash != SINGLETON_TOP_LAYER_V1_1_HASH.into() {
            return Err(DriverError::NonStandardLayer);
        }

        let conditions = Conditions::<NodePtr>::from_clvm(allocator, args.inner_puzzle)?;
        let (
            Some(Condition::AssertHeightAbsolute(assert_height_condition)),
            Some(Condition::CreateCoin(create_coin_condition)),
        ) = conditions
            .into_iter()
            .fold(
                (None, None),
                |(assert_height, create_coin), cond| match cond {
                    Condition::AssertHeightAbsolute(_) if assert_height.is_none() => {
                        (Some(cond), create_coin)
                    }
                    Condition::CreateCoin(_) if create_coin.is_none() => {
                        (assert_height, Some(cond))
                    }
                    _ => (assert_height, create_coin),
                },
            )
        else {
            return Err(DriverError::NonStandardLayer);
        };

        Ok(Some(Self {
            receiver_singleton_struct_hash: args.receiver_singleton_struct_hash,
            new_state_hash: args.message,
            required_block_height: assert_height_condition.height,
            new_puzzle_hash: create_coin_condition.puzzle_hash,
        }))
    }

    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        StateSchedulerLayerSolution::from_clvm(allocator, solution).map_err(DriverError::FromClvm)
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let base_conditions = Conditions::new()
            .create_coin(self.new_puzzle_hash, 1, Memos::None)
            .assert_height_absolute(self.required_block_height);

        let inner_puzzle = ctx.alloc(&clvm_quote!(base_conditions))?;

        ctx.curry(StateSchedulerLayerArgs::<Bytes32, NodePtr> {
            singleton_mod_hash: SINGLETON_TOP_LAYER_V1_1_HASH.into(),
            receiver_singleton_struct_hash: self.receiver_singleton_struct_hash,
            message: self.new_state_hash,
            inner_puzzle,
        })
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        ctx.alloc(&solution)
    }
}
