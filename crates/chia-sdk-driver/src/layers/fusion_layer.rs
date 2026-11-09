use std::collections::HashSet;

use chia_protocol::Bytes32;
use chia_sdk_types::{
    Mod,
    puzzles::{FUSION_SINGLETON_PUZZLE_HASH, FusionSingletonArgs, FusionSingletonSolution},
};
use clvm_traits::FromClvm;
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, P2FusionLayer, Puzzle, SpendContext};

/// The fusion [`Layer`] is the inner puzzle of the singleton from CHIP-0021. It swaps the NFTs on
/// side A for the NFTs on side B and vice versa, and holds neither itself. The locked NFTs are held
/// by the [`P2FusionLayer`], which only releases them when this singleton is spent.
///
/// By convention, side A is the fused or upgraded NFTs, and side B is the components or originals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FusionLayer {
    /// The launcher id of the fusion singleton.
    pub launcher_id: Bytes32,
    side_a: Vec<Bytes32>,
    side_b: Vec<Bytes32>,
}

impl FusionLayer {
    /// Creates the layer from the launcher ids of the NFTs on each side, which are in the order that
    /// they're curried into the puzzle.
    ///
    /// Each side must have at least one NFT, since otherwise the other side could be taken for free.
    /// An NFT can also only be listed once, since it couldn't be locked and unlocked at once.
    pub fn new(
        launcher_id: Bytes32,
        side_a: Vec<Bytes32>,
        side_b: Vec<Bytes32>,
    ) -> Result<Self, DriverError> {
        let unique: HashSet<&Bytes32> = side_a.iter().chain(&side_b).collect();

        if side_a.is_empty() || side_b.is_empty() || unique.len() != side_a.len() + side_b.len() {
            return Err(DriverError::InvalidFusion);
        }

        Ok(Self {
            launcher_id,
            side_a,
            side_b,
        })
    }

    /// The launcher ids of the NFTs on side A, which are unlocked by fusing.
    pub fn side_a(&self) -> &[Bytes32] {
        &self.side_a
    }

    /// The launcher ids of the NFTs on side B, which are unlocked by defusing.
    pub fn side_b(&self) -> &[Bytes32] {
        &self.side_b
    }

    /// The puzzle hash that NFTs are sent to in order to lock them.
    pub fn p2_puzzle_hash(&self) -> Bytes32 {
        P2FusionLayer::new(self.launcher_id).tree_hash().into()
    }

    fn args(&self) -> FusionSingletonArgs {
        FusionSingletonArgs::new(self.launcher_id, self.side_a.clone(), self.side_b.clone())
    }
}

impl Layer for FusionLayer {
    type Solution = FusionSingletonSolution;

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != FUSION_SINGLETON_PUZZLE_HASH {
            return Ok(None);
        }

        let args = FusionSingletonArgs::from_clvm(allocator, puzzle.args)?;
        let layer = Self::new(
            args.singleton_struct.launcher_id,
            args.nft_a_launcher_ids,
            args.nft_b_launcher_ids,
        )?;

        // The other curried arguments are constants, so a mismatch means that they aren't the
        // standard values, and NFTs couldn't be swapped with the spends that this layer creates.
        if layer.tree_hash() != puzzle.curried_puzzle_hash {
            return Err(DriverError::InvalidFusion);
        }

        Ok(Some(layer))
    }

    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        Ok(FusionSingletonSolution::from_clvm(allocator, solution)?)
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        ctx.curry(self.args())
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        ctx.alloc(&solution)
    }
}

impl ToTreeHash for FusionLayer {
    fn tree_hash(&self) -> TreeHash {
        self.args().curry_tree_hash()
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    const A: Bytes32 = Bytes32::new([1; 32]);
    const B: Bytes32 = Bytes32::new([2; 32]);

    #[rstest]
    #[case(&[], &[B])]
    #[case(&[A], &[])]
    #[case(&[A, A], &[B])]
    #[case(&[A], &[B, B])]
    #[case(&[A], &[A])]
    fn test_invalid_sides(#[case] side_a: &[Bytes32], #[case] side_b: &[Bytes32]) {
        assert!(matches!(
            FusionLayer::new(Bytes32::default(), side_a.to_vec(), side_b.to_vec()),
            Err(DriverError::InvalidFusion)
        ));
    }

    #[test]
    fn test_parse_round_trip() -> anyhow::Result<()> {
        let ctx = &mut SpendContext::new();

        let layer = FusionLayer::new(Bytes32::new([3; 32]), vec![A], vec![B])?;
        let puzzle = layer.construct_puzzle(ctx)?;
        assert_eq!(ctx.tree_hash(puzzle), layer.tree_hash());

        let puzzle = Puzzle::parse(ctx, puzzle);
        assert_eq!(FusionLayer::parse_puzzle(ctx, puzzle)?, Some(layer));

        Ok(())
    }

    #[test]
    fn test_parse_rejects_nonstandard_constants() -> anyhow::Result<()> {
        let ctx = &mut SpendContext::new();

        let puzzle = ctx.curry(FusionSingletonArgs {
            offer_mod_hash: Bytes32::default(),
            ..FusionSingletonArgs::new(Bytes32::new([3; 32]), vec![A], vec![B])
        })?;
        let puzzle = Puzzle::parse(ctx, puzzle);

        assert!(matches!(
            FusionLayer::parse_puzzle(ctx, puzzle),
            Err(DriverError::InvalidFusion)
        ));

        Ok(())
    }
}
