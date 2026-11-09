use chia_protocol::{Bytes32, Coin};
use chia_sdk_types::{Mod, puzzles::OptionContractArgs};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::Allocator;

use crate::{
    DriverError, Layer, OptionContractLayer, OptionUnderlying, Puzzle, SingletonInfo,
    SingletonLayer,
};

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

    /// Checks whether this option contract is backed by the given underlying coin.
    ///
    /// The option contract only commits to the underlying coin id and the hash of the delegated puzzle
    /// used to exercise it, so this should be checked before trusting an option from someone else.
    ///
    /// The `underlying_p2_puzzle_hash` is the hash of the puzzle locking the underlying coin, inside of
    /// any asset layers (such as the CAT, revocation, or NFT layers). For XCH, it's the coin's puzzle hash.
    pub fn is_backed_by(
        &self,
        underlying: &OptionUnderlying,
        underlying_coin: Coin,
        underlying_p2_puzzle_hash: Bytes32,
    ) -> bool {
        self.launcher_id == underlying.launcher_id
            && self.underlying_coin_id == underlying_coin.coin_id()
            && self.underlying_delegated_puzzle_hash
                == underlying.delegated_puzzle().tree_hash().into()
            && underlying_coin.amount == underlying.amount
            && underlying_p2_puzzle_hash == underlying.tree_hash().into()
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
}

impl SingletonInfo for OptionInfo {
    fn launcher_id(&self) -> Bytes32 {
        self.launcher_id
    }

    fn inner_puzzle_hash(&self) -> TreeHash {
        OptionContractArgs::new(
            self.underlying_coin_id,
            self.underlying_delegated_puzzle_hash,
            TreeHash::from(self.p2_puzzle_hash),
        )
        .curry_tree_hash()
    }
}
