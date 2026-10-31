use chia_protocol::Bytes32;
use chia_puzzles::{SINGLETON_LAUNCHER_HASH, SINGLETON_TOP_LAYER_HASH};
use chia_sdk_types::{
    Mod,
    puzzles::{P2SingletonClaimSolution, P2SingletonDelayedSolution, P2SingletonOrDelayedArgs},
};
use clvm_traits::FromClvm;
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, SpendContext, is_nil};

/// The pay to singleton or delayed puzzle hash [`Layer`] is what pool rewards are farmed to, and
/// what plots commit to. This is not the same puzzle as [`P2SingletonLayer`](crate::P2SingletonLayer),
/// and it pays to a singleton with the v1.0 top layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct P2SingletonOrDelayedLayer {
    /// The launcher id of the singleton that can claim coins.
    pub launcher_id: Bytes32,
    /// The number of seconds that a coin must exist before it can be sent to the delayed puzzle hash.
    pub seconds_delay: u64,
    /// The puzzle hash that a coin can be sent to after the delay, by anyone.
    pub delayed_puzzle_hash: Bytes32,
}

/// The ways that a [`P2SingletonOrDelayedLayer`] can be spent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum P2SingletonOrDelayedSolution {
    /// Claims the coin alongside a spend of the singleton.
    Claim(P2SingletonClaimSolution),
    /// Sends the coin to the delayed puzzle hash once it has existed for long enough.
    Delayed(P2SingletonDelayedSolution),
}

impl P2SingletonOrDelayedLayer {
    pub fn new(launcher_id: Bytes32, seconds_delay: u64, delayed_puzzle_hash: Bytes32) -> Self {
        Self {
            launcher_id,
            seconds_delay,
            delayed_puzzle_hash,
        }
    }

    fn args(&self) -> P2SingletonOrDelayedArgs {
        P2SingletonOrDelayedArgs::new(
            self.launcher_id,
            self.seconds_delay,
            self.delayed_puzzle_hash,
        )
    }
}

impl Layer for P2SingletonOrDelayedLayer {
    type Solution = P2SingletonOrDelayedSolution;

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != P2SingletonOrDelayedArgs::mod_hash() {
            return Ok(None);
        }

        let args = P2SingletonOrDelayedArgs::from_clvm(allocator, puzzle.args)?;

        if args.singleton_mod_hash != SINGLETON_TOP_LAYER_HASH.into()
            || args.launcher_puzzle_hash != SINGLETON_LAUNCHER_HASH.into()
        {
            return Ok(None);
        }

        Ok(Some(Self::new(
            args.launcher_id,
            args.seconds_delay,
            args.delayed_puzzle_hash,
        )))
    }

    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        let (p1, (my_id, _)) = <(NodePtr, (NodePtr, NodePtr))>::from_clvm(allocator, solution)?;

        Ok(if is_nil(allocator, my_id) {
            P2SingletonOrDelayedSolution::Delayed(P2SingletonDelayedSolution {
                amount: FromClvm::from_clvm(allocator, p1)?,
                my_id: (),
            })
        } else {
            P2SingletonOrDelayedSolution::Claim(P2SingletonClaimSolution {
                singleton_inner_puzzle_hash: FromClvm::from_clvm(allocator, p1)?,
                my_id: FromClvm::from_clvm(allocator, my_id)?,
            })
        })
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
            P2SingletonOrDelayedSolution::Claim(solution) => ctx.alloc(&solution),
            P2SingletonOrDelayedSolution::Delayed(solution) => ctx.alloc(&solution),
        }
    }
}

impl ToTreeHash for P2SingletonOrDelayedLayer {
    fn tree_hash(&self) -> TreeHash {
        self.args().curry_tree_hash()
    }
}
