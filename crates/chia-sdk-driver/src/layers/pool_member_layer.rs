use chia_bls::PublicKey;
use chia_protocol::Bytes32;
use chia_sdk_types::{
    Mod,
    puzzles::{PoolMemberAbsorbSolution, PoolMemberArgs, PoolMemberEscapeSolution},
};
use clvm_traits::FromClvm;
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr, SExp};

use crate::{DriverError, Layer, Puzzle, SpendContext};

/// The pool member [`Layer`] is the inner puzzle of a plot NFT that is farming to a pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoolMemberLayer {
    /// The puzzle hash that pool rewards are paid to, which is the pool's target puzzle hash.
    pub pool_puzzle_hash: Bytes32,
    /// The puzzle hash that pool rewards are farmed to.
    pub p2_singleton_puzzle_hash: Bytes32,
    /// The key which can start leaving the pool. This is not a synthetic key.
    pub owner_public_key: PublicKey,
    /// The first half of the genesis challenge followed by zeros, for recognizing pool rewards.
    pub pool_reward_prefix: Bytes32,
    /// The puzzle hash of the only inner puzzle that this one can move to.
    pub waiting_room_puzzle_hash: Bytes32,
}

/// The ways that a [`PoolMemberLayer`] can be spent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoolMemberSolution {
    /// Claims a pool reward, sending it to the pool. Anyone can do this.
    Absorb(PoolMemberAbsorbSolution),
    /// Moves to the waiting room puzzle, which the owner must sign.
    Escape { extra_data: NodePtr },
}

impl PoolMemberLayer {
    /// Returns the extra data if the inner solution moves to the waiting room, or [`None`] if it
    /// claims a reward. This is what the puzzle branches on, so unlike parsing the solution it
    /// works for any absorb that's valid on-chain, and anyone can choose how to solve those.
    pub fn parse_escape(
        allocator: &Allocator,
        inner_solution: NodePtr,
    ) -> Result<Option<NodePtr>, DriverError> {
        let (extra_data, (pool_reward_height, _)) =
            <(NodePtr, (NodePtr, NodePtr))>::from_clvm(allocator, inner_solution)?;

        Ok(is_nil(allocator, pool_reward_height).then_some(extra_data))
    }
}

pub(crate) fn is_nil(allocator: &Allocator, value: NodePtr) -> bool {
    matches!(allocator.sexp(value), SExp::Atom) && allocator.atom_len(value) == 0
}

impl Layer for PoolMemberLayer {
    type Solution = PoolMemberSolution;

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != PoolMemberArgs::mod_hash() {
            return Ok(None);
        }

        let args = PoolMemberArgs::from_clvm(allocator, puzzle.args)?;

        Ok(Some(Self {
            pool_puzzle_hash: args.pool_puzzle_hash,
            p2_singleton_puzzle_hash: args.p2_singleton_puzzle_hash,
            owner_public_key: args.owner_public_key,
            pool_reward_prefix: args.pool_reward_prefix,
            waiting_room_puzzle_hash: args.waiting_room_puzzle_hash,
        }))
    }

    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        if let Some(extra_data) = Self::parse_escape(allocator, solution)? {
            return Ok(PoolMemberSolution::Escape { extra_data });
        }

        let (pool_reward_amount, (pool_reward_height, _)) =
            <(u64, (u32, NodePtr))>::from_clvm(allocator, solution)?;

        Ok(PoolMemberSolution::Absorb(PoolMemberAbsorbSolution {
            pool_reward_amount,
            pool_reward_height,
        }))
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        ctx.curry(self.args())
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        match solution {
            PoolMemberSolution::Absorb(solution) => {
                if solution.pool_reward_height == 0 {
                    return Err(DriverError::InvalidPoolReward(
                        "the pool member puzzle can't claim a reward from height 0",
                    ));
                }
                ctx.alloc(&solution)
            }
            PoolMemberSolution::Escape { extra_data } => ctx.alloc(&PoolMemberEscapeSolution {
                extra_data,
                pool_reward_height: (),
            }),
        }
    }
}

impl PoolMemberLayer {
    fn args(&self) -> PoolMemberArgs {
        PoolMemberArgs {
            pool_puzzle_hash: self.pool_puzzle_hash,
            p2_singleton_puzzle_hash: self.p2_singleton_puzzle_hash,
            owner_public_key: self.owner_public_key,
            pool_reward_prefix: self.pool_reward_prefix,
            waiting_room_puzzle_hash: self.waiting_room_puzzle_hash,
        }
    }
}

impl ToTreeHash for PoolMemberLayer {
    fn tree_hash(&self) -> TreeHash {
        self.args().curry_tree_hash()
    }
}
