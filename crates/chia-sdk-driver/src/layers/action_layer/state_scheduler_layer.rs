use chia::{
    clvm_utils::{CurriedProgram, ToTreeHash, TreeHash},
    protocol::Bytes32,
};
use chia_puzzle_types::Memos;
use chia_puzzles::SINGLETON_TOP_LAYER_V1_1_HASH;
use chia_wallet_sdk::{
    driver::{DriverError, Layer, Puzzle, SpendContext},
    types::{Condition, Conditions},
};
use clvm_traits::{clvm_quote, FromClvm, ToClvm};
use clvmr::{Allocator, NodePtr};
use hex_literal::hex;

use crate::SpendContextExt;

#[derive(Debug, Clone, PartialEq, Eq)]
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

        CurriedProgram {
            program: ctx.state_scheduler_puzzle()?,
            args: StateSchedulerLayerArgs::<Bytes32, NodePtr> {
                singleton_mod_hash: SINGLETON_TOP_LAYER_V1_1_HASH.into(),
                receiver_singleton_struct_hash: self.receiver_singleton_struct_hash,
                message: self.new_state_hash,
                inner_puzzle,
            },
        }
        .to_clvm(ctx)
        .map_err(DriverError::ToClvm)
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        ctx.alloc(&solution)
    }
}

pub const STATE_SCHEDULER_PUZZLE: [u8; 285] = hex!("ff02ffff01ff04ffff04ff04ffff04ffff0112ffff04ff17ffff04ffff0bff2effff0bff0affff0bff0aff36ff0580ffff0bff0affff0bff3effff0bff0affff0bff0aff36ff0b80ffff0bff0affff0bff3effff0bff0affff0bff0aff36ff5f80ffff0bff0aff36ff26808080ff26808080ff26808080ff8080808080ffff02ff2fff7f8080ffff04ffff01ff42ff02ffffa04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f596718ba7b2ffa102a12871fee210fb8619291eaea194581cbd2531e4b23759d225f6806923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ff018080");

pub const STATE_SCHEDULER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    13fe7833751a6fe582caa09d48978d8d1b016d224cb0c10e538184ab22df9c13
    "
));

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(curry)]
pub struct StateSchedulerLayerArgs<M, I> {
    pub singleton_mod_hash: Bytes32,
    pub receiver_singleton_struct_hash: Bytes32,
    pub message: M,
    pub inner_puzzle: I,
}

impl<M, I> StateSchedulerLayerArgs<M, I>
where
    M: ToTreeHash,
    I: ToTreeHash,
{
    pub fn curry_tree_hash(
        receiver_singleton_struct_hash: Bytes32,
        message: M,
        inner_puzzle: I,
    ) -> TreeHash {
        CurriedProgram::<TreeHash, _> {
            program: STATE_SCHEDULER_PUZZLE_HASH,
            args: StateSchedulerLayerArgs {
                singleton_mod_hash: SINGLETON_TOP_LAYER_V1_1_HASH.into(),
                receiver_singleton_struct_hash,
                message: message.tree_hash(),
                inner_puzzle: inner_puzzle.tree_hash(),
            },
        }
        .tree_hash()
    }
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct StateSchedulerLayerSolution<I> {
    pub other_singleton_inner_puzzle_hash: Bytes32,
    #[clvm(rest)]
    pub inner_solution: I,
}
