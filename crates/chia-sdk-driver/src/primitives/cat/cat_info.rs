use chia_protocol::Bytes32;
use chia_puzzle_types::cat::CatArgs;
use chia_sdk_types::{puzzles::RevocationArgs, Mod};
use clvm_utils::TreeHash;
use clvmr::{Allocator, NodePtr};

use crate::{CatLayer, DriverError, Layer, Puzzle, RevocationLayer, SpendContext};

/// Information needed to construct the outer puzzle of a CAT.
/// This includes the [`CatLayer`] and [`RevocationLayer`] if present.
/// However, it does not include the inner puzzle, which must be stored separately.
///
/// This type can be used on its own for parsing, or as part of the [`Cat`](crate::Cat) primitive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatInfo {
    /// The hash of the TAIL (Token and Asset Issuance Limitations) program.
    /// This is what controls the supply, and thus the main way to identify a CAT.
    /// You can spend multiple CAT coins at once, as long as they have the same [`asset_id`](Self::asset_id).
    pub asset_id: Bytes32,

    /// The hash of the hidden puzzle, if this is a revocable CAT.
    /// A revocable CAT is one in which the inner puzzle is wrapped in the [`RevocationLayer`].
    pub hidden_puzzle_hash: Option<Bytes32>,

    /// The hash of the inner puzzle to this CAT. For revocable CATs, it's the inner puzzle of the [`RevocationLayer`].
    /// If you encode this puzzle hash as bech32m, it's the same as the current owner's address.
    pub p2_puzzle_hash: Bytes32,
}

impl CatInfo {
    pub fn new(
        asset_id: Bytes32,
        hidden_puzzle_hash: Option<Bytes32>,
        p2_puzzle_hash: Bytes32,
    ) -> Self {
        Self {
            asset_id,
            hidden_puzzle_hash,
            p2_puzzle_hash,
        }
    }

    /// Parses a [`CatInfo`] from a [`Puzzle`] by extracting the [`CatLayer`] and [`RevocationLayer`] if present.
    ///
    /// This will return a tuple of the [`CatInfo`] and its p2 puzzle. If the CAT is
    /// revocable, the p2 puzzle will be [`None`], since it's not revealed until the CAT is spent.
    ///
    /// If the puzzle is not a CAT, this will return [`None`] instead of an error.
    /// However, if the puzzle should have been a CAT but had a parsing error, this will return an error.
    pub fn parse(
        allocator: &Allocator,
        puzzle: Puzzle,
    ) -> Result<Option<(Self, Option<Puzzle>)>, DriverError> {
        let Some(cat_layer) = CatLayer::<Puzzle>::parse_puzzle(allocator, puzzle)? else {
            return Ok(None);
        };

        if let Some(revocation_layer) =
            RevocationLayer::parse_puzzle(allocator, cat_layer.inner_puzzle)?
        {
            let info = Self::new(
                cat_layer.asset_id,
                Some(revocation_layer.hidden_puzzle_hash),
                revocation_layer.inner_puzzle_hash,
            );
            Ok(Some((info, None)))
        } else {
            let info = Self::new(
                cat_layer.asset_id,
                None,
                cat_layer.inner_puzzle.curried_puzzle_hash().into(),
            );
            Ok(Some((info, Some(cat_layer.inner_puzzle))))
        }
    }

    /// Calculates the inner puzzle hash of the CAT.
    ///
    /// This is only different than the [`p2_puzzle_hash`](Self::p2_puzzle_hash) for revocable CATs.
    pub fn inner_puzzle_hash(&self) -> TreeHash {
        let mut inner_puzzle_hash = TreeHash::from(self.p2_puzzle_hash);

        if let Some(hidden_puzzle_hash) = self.hidden_puzzle_hash {
            inner_puzzle_hash =
                RevocationArgs::new(hidden_puzzle_hash, inner_puzzle_hash.into()).curry_tree_hash();
        }

        inner_puzzle_hash
    }

    /// Calculates the full puzzle hash of the CAT, which is the hash of the outer [`CatLayer`].
    pub fn puzzle_hash(&self) -> TreeHash {
        CatArgs::curry_tree_hash(self.asset_id, self.inner_puzzle_hash())
    }

    /// Calculates the full puzzle of the CAT. If the CAT is revocable, the [`Self::p2_puzzle_hash`]
    /// if used instead of the passed in p2 puzzle reveal. This is because the revocation layer
    /// reveals the inner puzzle in the solution instead of the puzzle.
    pub fn construct_puzzle(
        &self,
        ctx: &mut SpendContext,
        p2_puzzle: NodePtr,
    ) -> Result<NodePtr, DriverError> {
        let mut inner_puzzle = p2_puzzle;

        if let Some(hidden_puzzle_hash) = self.hidden_puzzle_hash {
            inner_puzzle =
                ctx.curry(RevocationArgs::new(hidden_puzzle_hash, self.p2_puzzle_hash))?;
        }

        ctx.curry(CatArgs::new(self.asset_id, inner_puzzle))
    }
}
