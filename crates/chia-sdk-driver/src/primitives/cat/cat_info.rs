use chia_protocol::Bytes32;
use chia_puzzle_types::cat::CatArgs;
use chia_sdk_types::{puzzles::RevocationArgs, Mod};
use clvm_utils::TreeHash;
use clvmr::Allocator;

use crate::{CatLayer, DriverError, Layer, Puzzle, RevocationLayer};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatInfo {
    pub asset_id: Bytes32,
    pub hidden_puzzle_hash: Option<Bytes32>,
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

    pub fn parse(
        &self,
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

    pub fn inner_puzzle_hash(&self) -> TreeHash {
        let mut inner_puzzle_hash = TreeHash::from(self.p2_puzzle_hash);
        if let Some(hidden_puzzle_hash) = self.hidden_puzzle_hash {
            inner_puzzle_hash =
                RevocationArgs::new(hidden_puzzle_hash, inner_puzzle_hash.into()).curry_tree_hash();
        }
        inner_puzzle_hash
    }

    pub fn puzzle_hash(&self) -> TreeHash {
        CatArgs::curry_tree_hash(self.asset_id, self.inner_puzzle_hash())
    }
}
