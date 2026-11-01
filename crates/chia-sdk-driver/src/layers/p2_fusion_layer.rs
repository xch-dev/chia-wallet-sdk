use chia_protocol::Bytes32;
use chia_puzzles::{SINGLETON_LAUNCHER_HASH, SINGLETON_TOP_LAYER_V1_1_HASH};
use chia_sdk_types::{
    Mod,
    puzzles::{P2_FUSION_PUZZLE_HASH, P2FusionArgs, P2FusionSolution},
};
use clvm_traits::FromClvm;
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, Spend, SpendContext};

/// The p2 fusion [`Layer`] is the p2 puzzle of NFTs locked by a [`FusionLayer`](crate::FusionLayer)
/// singleton. It can only be spent by NFTs, and only if the singleton announces the NFT's release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct P2FusionLayer {
    /// The launcher id of the fusion singleton.
    pub launcher_id: Bytes32,
}

impl P2FusionLayer {
    pub fn new(launcher_id: Bytes32) -> Self {
        Self { launcher_id }
    }

    pub fn spend(
        &self,
        ctx: &mut SpendContext,
        solution: P2FusionSolution,
    ) -> Result<Spend, DriverError> {
        self.construct_spend(ctx, solution)
    }
}

impl Layer for P2FusionLayer {
    type Solution = P2FusionSolution;

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != P2_FUSION_PUZZLE_HASH {
            return Ok(None);
        }

        let args = P2FusionArgs::from_clvm(allocator, puzzle.args)?;

        if args.singleton_mod_hash != SINGLETON_TOP_LAYER_V1_1_HASH.into()
            || args.launcher_puzzle_hash != SINGLETON_LAUNCHER_HASH.into()
        {
            return Err(DriverError::InvalidSingletonStruct);
        }

        Ok(Some(Self::new(args.launcher_id)))
    }

    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        Ok(P2FusionSolution::from_clvm(allocator, solution)?)
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        ctx.curry(P2FusionArgs::new(self.launcher_id))
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        ctx.alloc(&solution)
    }
}

impl ToTreeHash for P2FusionLayer {
    fn tree_hash(&self) -> TreeHash {
        P2FusionArgs::new(self.launcher_id).curry_tree_hash()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_round_trip() -> anyhow::Result<()> {
        let ctx = &mut SpendContext::new();

        let layer = P2FusionLayer::new(Bytes32::new([1; 32]));
        let puzzle = layer.construct_puzzle(ctx)?;
        assert_eq!(ctx.tree_hash(puzzle), layer.tree_hash());

        let puzzle = Puzzle::parse(ctx, puzzle);
        assert_eq!(P2FusionLayer::parse_puzzle(ctx, puzzle)?, Some(layer));

        Ok(())
    }

    #[test]
    fn test_parse_rejects_nonstandard_singleton() -> anyhow::Result<()> {
        let ctx = &mut SpendContext::new();

        let puzzle = ctx.curry(P2FusionArgs {
            singleton_mod_hash: Bytes32::default(),
            ..P2FusionArgs::new(Bytes32::new([1; 32]))
        })?;
        let puzzle = Puzzle::parse(ctx, puzzle);

        assert!(matches!(
            P2FusionLayer::parse_puzzle(ctx, puzzle),
            Err(DriverError::InvalidSingletonStruct)
        ));

        Ok(())
    }
}
