use chia_protocol::Bytes32;
use chia_puzzles::nft::{NftOwnershipLayerArgs, NftStateLayerArgs};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{tree_hash, ToTreeHash, TreeHash};
use clvmr::Allocator;

use crate::{
    DriverError, Layer, NftOwnershipLayer, NftStateLayer, Puzzle, RoyaltyTransferLayer,
    SingletonLayer,
};

pub type StandardNftLayers<M, I> =
    SingletonLayer<NftStateLayer<M, NftOwnershipLayer<RoyaltyTransferLayer, I>>>;

#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NftInfo<M> {
    pub launcher_id: Bytes32,
    pub metadata: M,
    pub metadata_updater_puzzle_hash: Bytes32,
    pub current_owner: Option<Bytes32>,
    pub royalty_puzzle_hash: Bytes32,
    pub royalty_ten_thousandths: u16,
    pub p2_puzzle_hash: Bytes32,
}

impl<M> NftInfo<M> {
    pub fn new(
        launcher_id: Bytes32,
        metadata: M,
        metadata_updater_puzzle_hash: Bytes32,
        current_owner: Option<Bytes32>,
        royalty_puzzle_hash: Bytes32,
        royalty_ten_thousandths: u16,
        p2_puzzle_hash: Bytes32,
    ) -> Self {
        Self {
            launcher_id,
            metadata,
            metadata_updater_puzzle_hash,
            current_owner,
            royalty_puzzle_hash,
            royalty_ten_thousandths,
            p2_puzzle_hash,
        }
    }

    /// Parses the NFT info and p2 puzzle that corresponds to the p2 puzzle hash.
    pub fn parse(
        allocator: &Allocator,
        puzzle: Puzzle,
    ) -> Result<Option<(Self, Puzzle)>, DriverError>
    where
        M: ToClvm<Allocator> + FromClvm<Allocator>,
    {
        let Some(layers) = StandardNftLayers::<M, Puzzle>::parse_puzzle(allocator, puzzle)? else {
            return Ok(None);
        };

        let p2_puzzle = layers.inner_puzzle.inner_puzzle.inner_puzzle;

        Ok(Some((Self::from_layers(layers), p2_puzzle)))
    }

    pub fn from_layers<I>(layers: StandardNftLayers<M, I>) -> Self
    where
        I: ToTreeHash,
    {
        Self {
            launcher_id: layers.launcher_id,
            metadata: layers.inner_puzzle.metadata,
            metadata_updater_puzzle_hash: layers.inner_puzzle.metadata_updater_puzzle_hash,
            current_owner: layers.inner_puzzle.inner_puzzle.current_owner,
            royalty_puzzle_hash: layers
                .inner_puzzle
                .inner_puzzle
                .transfer_layer
                .royalty_puzzle_hash,
            royalty_ten_thousandths: layers
                .inner_puzzle
                .inner_puzzle
                .transfer_layer
                .royalty_ten_thousandths,
            p2_puzzle_hash: layers
                .inner_puzzle
                .inner_puzzle
                .inner_puzzle
                .tree_hash()
                .into(),
        }
    }

    #[must_use]
    pub fn into_layers<I>(self, p2_puzzle: I) -> StandardNftLayers<M, I> {
        SingletonLayer::new(
            self.launcher_id,
            NftStateLayer::new(
                self.metadata,
                self.metadata_updater_puzzle_hash,
                NftOwnershipLayer::new(
                    self.current_owner,
                    RoyaltyTransferLayer::new(
                        self.launcher_id,
                        self.royalty_puzzle_hash,
                        self.royalty_ten_thousandths,
                    ),
                    p2_puzzle,
                ),
            ),
        )
    }

    pub fn with_metadata<N>(self, metadata: N) -> NftInfo<N> {
        NftInfo {
            launcher_id: self.launcher_id,
            metadata,
            metadata_updater_puzzle_hash: self.metadata_updater_puzzle_hash,
            current_owner: self.current_owner,
            royalty_puzzle_hash: self.royalty_puzzle_hash,
            royalty_ten_thousandths: self.royalty_ten_thousandths,
            p2_puzzle_hash: self.p2_puzzle_hash,
        }
    }

    pub fn inner_puzzle_hash(&self, allocator: &mut Allocator) -> Result<TreeHash, DriverError>
    where
        M: ToClvm<Allocator>,
    {
        let metadata_ptr = self.metadata.to_clvm(allocator)?;

        Ok(NftStateLayerArgs::curry_tree_hash(
            tree_hash(allocator, metadata_ptr),
            NftOwnershipLayerArgs::curry_tree_hash(
                self.current_owner,
                RoyaltyTransferLayer::new(
                    self.launcher_id,
                    self.royalty_puzzle_hash,
                    self.royalty_ten_thousandths,
                )
                .tree_hash(),
                self.p2_puzzle_hash.into(),
            ),
        ))
    }
}
