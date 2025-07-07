use chia_protocol::Bytes32;
use chia_puzzle_types::singleton::SingletonArgs;
use chia_sdk_types::{puzzles::OptionContractArgs, Mod};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::Allocator;

use crate::{DriverError, Layer, OptionContractLayer, Puzzle, SingletonLayer};

pub type OptionContractLayers<I> = SingletonLayer<OptionContractLayer<I>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OptionInfo {
    pub launcher_id: Bytes32,
    pub underlying_coin_id: Bytes32,
    pub underlying_delegated_puzzle_hash: Bytes32,
    pub p2_puzzle_hash: Bytes32,
}

impl OptionInfo {
    pub fn new(
        launcher_id: Bytes32,
        underlying_coin_id: Bytes32,
        underlying_delegated_puzzle_hash: Bytes32,
        p2_puzzle_hash: Bytes32,
    ) -> Self {
        Self {
            launcher_id,
            underlying_coin_id,
            underlying_delegated_puzzle_hash,
            p2_puzzle_hash,
        }
    }

    pub fn parse(
        allocator: &Allocator,
        puzzle: Puzzle,
    ) -> Result<Option<(Self, Puzzle)>, DriverError> {
        let Some(layers) = OptionContractLayers::parse_puzzle(allocator, puzzle)? else {
            return Ok(None);
        };

        let p2_puzzle = layers.inner_puzzle.inner_puzzle;

        Ok(Some((Self::from_layers(&layers), p2_puzzle)))
    }

    pub fn from_layers<I>(layers: &OptionContractLayers<I>) -> Self
    where
        I: ToTreeHash,
    {
        Self {
            launcher_id: layers.launcher_id,
            underlying_coin_id: layers.inner_puzzle.underlying_coin_id,
            underlying_delegated_puzzle_hash: layers.inner_puzzle.underlying_delegated_puzzle_hash,
            p2_puzzle_hash: layers.inner_puzzle.inner_puzzle.tree_hash().into(),
        }
    }

    #[must_use]
    pub fn into_layers<I>(self, p2_puzzle: I) -> OptionContractLayers<I> {
        SingletonLayer::new(
            self.launcher_id,
            OptionContractLayer::new(
                self.underlying_coin_id,
                self.underlying_delegated_puzzle_hash,
                p2_puzzle,
            ),
        )
    }

    pub fn puzzle_hash(&self) -> TreeHash {
        SingletonArgs::curry_tree_hash(self.launcher_id, self.inner_puzzle_hash())
    }

    pub fn inner_puzzle_hash(&self) -> TreeHash {
        OptionContractArgs::new(
            self.underlying_coin_id,
            self.underlying_delegated_puzzle_hash,
            TreeHash::from(self.p2_puzzle_hash),
        )
        .curry_tree_hash()
    }

    #[must_use]
    pub fn with_p2_puzzle_hash(self, p2_puzzle_hash: Bytes32) -> Self {
        Self {
            launcher_id: self.launcher_id,
            underlying_coin_id: self.underlying_coin_id,
            underlying_delegated_puzzle_hash: self.underlying_delegated_puzzle_hash,
            p2_puzzle_hash,
        }
    }
}
