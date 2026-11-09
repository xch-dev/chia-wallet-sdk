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

/// A puzzle that maps an inner puzzle hash to the full puzzle hash of a payment coin.
///
/// Puzzles such as the action-layer apps, partial offers, and the p2 parent puzzle use CAT makers
/// so the asset can be configured: the puzzle curries or stores a CAT maker's puzzle hash, and runs
/// the CAT maker to find the puzzle hash that a coin of the asset (for example a `PrecommitCoin`)
/// must have.
///
/// The solution of a CAT maker is `(inner_puzzle_hash . solution_rest)`.
///
/// # Example
///
/// ```
/// use chia_protocol::Bytes32;
/// use chia_puzzle_types::cat::CatArgs;
/// use chia_sdk_driver::{CatMaker, SpendContext};
/// use clvm_utils::ToTreeHash;
///
/// let ctx = &mut SpendContext::new();
///
/// let asset_id = Bytes32::new([1; 32]);
/// let inner_puzzle_hash = Bytes32::new([2; 32]);
///
/// let cat_maker = CatMaker::Default {
///     tail_hash_hash: asset_id.tree_hash(),
/// };
/// let puzzle_hash = cat_maker.run(ctx, inner_puzzle_hash, ())?;
/// assert_eq!(
///     puzzle_hash,
///     CatArgs::curry_tree_hash(asset_id, inner_puzzle_hash.into()).into()
/// );
///
/// // The XCH CAT maker returns the inner puzzle hash unchanged.
/// assert_eq!(CatMaker::Xch.run(ctx, inner_puzzle_hash, ())?, inner_puzzle_hash);
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatMaker {
    /// Makes standard (non-revocable) CAT puzzle hashes.
    Default {
        /// The tree hash of the CAT's asset id (TAIL hash), as in `asset_id.tree_hash()`.
        tail_hash_hash: TreeHash,
    },
    /// Makes revocable CAT puzzle hashes, which wrap the inner puzzle in the revocation layer.
    Revocable {
        /// The tree hash of the CAT's asset id (TAIL hash), as in `asset_id.tree_hash()`.
        tail_hash_hash: TreeHash,
        /// The tree hash of the revocation layer's hidden puzzle hash.
        hidden_puzzle_hash_hash: TreeHash,
    },
    /// Returns the inner puzzle hash unchanged, for payments in XCH.
    Xch,
}

impl CatMaker {
    /// Returns the tree hash of the CAT maker puzzle, which is what apps store or curry.
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

    /// Allocates the CAT maker puzzle.
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

    /// Runs the CAT maker on `inner_puzzle_hash` and returns the resulting puzzle hash.
    ///
    /// `solution_rest` is appended after the inner puzzle hash in the solution. The built-in
    /// CAT makers don't use it, so pass `()`.
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

    /// Parses one of the known CAT maker puzzles, returning [`None`] if it doesn't match any of them.
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
