use chia_protocol::Bytes32;
use chia_puzzles::SINGLETON_TOP_LAYER_V1_1_HASH;
use chia_sdk_types::puzzles::{
    P2_DELEGATED_BY_SINGLETON_PUZZLE_HASH, P2DelegatedBySingletonLayerArgs,
    P2DelegatedBySingletonLayerSolution,
};
use clvm_traits::{FromClvm, ToClvm};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, SpendContext};

/// The p2 delegated by singleton [`Layer`] lets a singleton spend a coin by approving a delegated
/// puzzle with a message.
///
/// The puzzle ([`P2DelegatedBySingletonLayerArgs`]) outputs a `RECEIVE_MESSAGE` condition with
/// mode 23, which requires the singleton identified by
/// [`singleton_struct_hash`](Self::singleton_struct_hash) to send this coin a message equal to
/// the tree hash of the delegated puzzle. It then runs the delegated puzzle with the delegated
/// solution. The solution ([`P2DelegatedBySingletonLayerSolution`]) includes the singleton's
/// current inner puzzle hash, which is used to compute its full puzzle hash.
///
/// Action layer apps use it for coins they control, such as a [`Reserve`](crate::Reserve) or
/// the NFTs and CATs locked in a reward distributor.
///
/// # Example
///
/// ```
/// use chia_protocol::Bytes32;
/// use chia_puzzle_types::singleton::SingletonStruct;
/// use chia_sdk_driver::{Layer, P2DelegatedBySingletonLayer, Puzzle, SpendContext};
/// use chia_sdk_types::puzzles::P2DelegatedBySingletonLayerArgs;
/// use clvm_utils::ToTreeHash;
///
/// let ctx = &mut SpendContext::new();
///
/// let launcher_id = Bytes32::new([1; 32]);
/// let singleton_struct_hash: Bytes32 = SingletonStruct::new(launcher_id).tree_hash().into();
/// let layer = P2DelegatedBySingletonLayer::new(singleton_struct_hash, 0);
///
/// let puzzle = layer.construct_puzzle(ctx)?;
/// assert_eq!(
///     ctx.tree_hash(puzzle),
///     P2DelegatedBySingletonLayerArgs::curry_tree_hash_with_launcher_id(launcher_id, 0)
/// );
///
/// let parsed = P2DelegatedBySingletonLayer::parse_puzzle(ctx, Puzzle::parse(ctx, puzzle))?;
/// assert_eq!(parsed, Some(layer));
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub struct P2DelegatedBySingletonLayer {
    /// The tree hash of the controlling singleton's [`SingletonStruct`](chia_puzzle_types::singleton::SingletonStruct).
    pub singleton_struct_hash: Bytes32,
    /// A nonce that isn't used by the puzzle, but makes it possible for the same singleton to
    /// control coins with different puzzle hashes.
    pub nonce: u64,
}

impl P2DelegatedBySingletonLayer {
    /// Creates a new [`P2DelegatedBySingletonLayer`] controlled by the singleton with the given
    /// struct hash.
    pub fn new(singleton_struct_hash: Bytes32, nonce: u64) -> Self {
        Self {
            singleton_struct_hash,
            nonce,
        }
    }
}

impl Layer for P2DelegatedBySingletonLayer {
    type Solution = P2DelegatedBySingletonLayerSolution<NodePtr, NodePtr>;

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != P2_DELEGATED_BY_SINGLETON_PUZZLE_HASH {
            return Ok(None);
        }

        let args = P2DelegatedBySingletonLayerArgs::from_clvm(allocator, puzzle.args)?;

        if args.singleton_mod_hash != SINGLETON_TOP_LAYER_V1_1_HASH.into() {
            return Ok(None);
        }

        Ok(Some(Self {
            singleton_struct_hash: args.singleton_struct_hash,
            nonce: args.nonce,
        }))
    }

    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        P2DelegatedBySingletonLayerSolution::from_clvm(allocator, solution)
            .map_err(DriverError::FromClvm)
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        ctx.curry(P2DelegatedBySingletonLayerArgs::new(
            self.singleton_struct_hash,
            self.nonce,
        ))
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        solution.to_clvm(ctx).map_err(DriverError::ToClvm)
    }
}
