use chia_bls::PublicKey;
use chia_protocol::Bytes32;
use chia_sdk_types::{
    Mod,
    puzzles::{PoolWaitingRoomAbsorbSolution, PoolWaitingRoomArgs, PoolWaitingRoomTravelSolution},
};
use clvm_traits::FromClvm;
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, SpendContext, is_nil, parse_pool_reward_height};

/// The pool waiting room [`Layer`] is the inner puzzle of a plot NFT that is self pooling or leaving
/// a pool. Self pooling is the same puzzle with a [`relative_lock_height`](Self::relative_lock_height)
/// of 0, and with the owner's own puzzle hash as the [`pool_puzzle_hash`](Self::pool_puzzle_hash).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoolWaitingRoomLayer {
    /// The puzzle hash that pool rewards are paid to.
    pub pool_puzzle_hash: Bytes32,
    /// The puzzle hash that pool rewards are farmed to.
    pub p2_singleton_puzzle_hash: Bytes32,
    /// The key which can move the singleton to a new puzzle. This is not a synthetic key.
    pub owner_public_key: PublicKey,
    /// The first half of the genesis challenge followed by zeros, for recognizing pool rewards.
    pub pool_reward_prefix: Bytes32,
    /// The number of blocks that the coin must exist before it can move to a new puzzle.
    pub relative_lock_height: u32,
}

/// The ways that a [`PoolWaitingRoomLayer`] can be spent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoolWaitingRoomSolution {
    /// Claims a pool reward, sending it to the pool. Anyone can do this.
    Absorb(PoolWaitingRoomAbsorbSolution),
    /// Moves to the destination puzzle hash, which the owner must sign.
    Travel {
        extra_data: NodePtr,
        destination_puzzle_hash: Bytes32,
    },
}

impl PoolWaitingRoomLayer {
    /// Returns the extra data and destination if the inner solution moves to a new puzzle, or
    /// [`None`] if it claims a reward. This is what the puzzle branches on, and it works for any
    /// spend that's valid on-chain. The destination is left unparsed since the singleton accepts
    /// any atom, even though only a 32 byte one is a puzzle hash.
    pub fn parse_travel(
        allocator: &Allocator,
        inner_solution: NodePtr,
    ) -> Result<Option<(NodePtr, NodePtr)>, DriverError> {
        let (spend_type, rest) = <(NodePtr, NodePtr)>::from_clvm(allocator, inner_solution)?;

        if is_nil(allocator, spend_type) {
            return Ok(None);
        }

        let (extra_data, (destination, _)) =
            <(NodePtr, (NodePtr, NodePtr))>::from_clvm(allocator, rest)?;

        Ok(Some((extra_data, destination)))
    }

    fn args(&self) -> PoolWaitingRoomArgs {
        PoolWaitingRoomArgs {
            pool_puzzle_hash: self.pool_puzzle_hash,
            p2_singleton_puzzle_hash: self.p2_singleton_puzzle_hash,
            owner_public_key: self.owner_public_key,
            pool_reward_prefix: self.pool_reward_prefix,
            relative_lock_height: self.relative_lock_height,
        }
    }
}

impl Layer for PoolWaitingRoomLayer {
    type Solution = PoolWaitingRoomSolution;

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != PoolWaitingRoomArgs::mod_hash() {
            return Ok(None);
        }

        let args = PoolWaitingRoomArgs::from_clvm(allocator, puzzle.args)?;

        Ok(Some(Self {
            pool_puzzle_hash: args.pool_puzzle_hash,
            p2_singleton_puzzle_hash: args.p2_singleton_puzzle_hash,
            owner_public_key: args.owner_public_key,
            pool_reward_prefix: args.pool_reward_prefix,
            relative_lock_height: args.relative_lock_height,
        }))
    }

    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        if let Some((extra_data, destination)) = Self::parse_travel(allocator, solution)? {
            return Ok(PoolWaitingRoomSolution::Travel {
                extra_data,
                destination_puzzle_hash: Bytes32::from_clvm(allocator, destination)?,
            });
        }

        let (_, (pool_reward_amount, (pool_reward_height, _))) =
            <(NodePtr, (u64, (NodePtr, NodePtr)))>::from_clvm(allocator, solution)?;

        Ok(PoolWaitingRoomSolution::Absorb(
            PoolWaitingRoomAbsorbSolution {
                spend_type: (),
                pool_reward_amount,
                pool_reward_height: parse_pool_reward_height(allocator, pool_reward_height)?,
            },
        ))
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
            PoolWaitingRoomSolution::Absorb(solution) => ctx.alloc(&solution),
            PoolWaitingRoomSolution::Travel {
                extra_data,
                destination_puzzle_hash,
            } => ctx.alloc(&PoolWaitingRoomTravelSolution {
                spend_type: 1,
                extra_data,
                destination_puzzle_hash,
            }),
        }
    }
}

impl ToTreeHash for PoolWaitingRoomLayer {
    fn tree_hash(&self) -> TreeHash {
        self.args().curry_tree_hash()
    }
}
