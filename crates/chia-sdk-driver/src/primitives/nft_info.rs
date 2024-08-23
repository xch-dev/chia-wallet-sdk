use chia_protocol::Bytes32;
use chia_puzzles::nft::{
    NftOwnershipLayerArgs, NftRoyaltyTransferPuzzleArgs, NftStateLayerArgs,
    NFT_ROYALTY_TRANSFER_PUZZLE_HASH,
};
use clvm_traits::ToClvm;
use clvm_utils::{tree_hash, CurriedProgram, ToTreeHash, TreeHash};
use clvmr::Allocator;

use crate::{DriverError, NftOwnershipLayer, NftStateLayer, RoyaltyTransferLayer, SingletonLayer};

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

    pub fn inner_puzzle_hash(&self) -> TreeHash
    where
        M: ToTreeHash,
    {
        NftStateLayerArgs::curry_tree_hash(
            self.metadata.tree_hash(),
            NftOwnershipLayerArgs::curry_tree_hash(
                self.current_owner,
                CurriedProgram {
                    program: NFT_ROYALTY_TRANSFER_PUZZLE_HASH,
                    args: NftRoyaltyTransferPuzzleArgs::new(
                        self.launcher_id,
                        self.royalty_puzzle_hash,
                        self.royalty_ten_thousandths,
                    ),
                }
                .tree_hash(),
                self.p2_puzzle_hash.into(),
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

    pub fn with_hashed_metadata(
        &self,
        allocator: &mut Allocator,
    ) -> Result<NftInfo<TreeHash>, DriverError>
    where
        M: ToClvm<Allocator>,
    {
        let metadata_ptr = self.metadata.to_clvm(allocator)?;
        let metadata_hash = tree_hash(allocator, metadata_ptr);
        Ok(NftInfo {
            launcher_id: self.launcher_id,
            metadata: metadata_hash,
            metadata_updater_puzzle_hash: self.metadata_updater_puzzle_hash,
            current_owner: self.current_owner,
            royalty_puzzle_hash: self.royalty_puzzle_hash,
            royalty_ten_thousandths: self.royalty_ten_thousandths,
            p2_puzzle_hash: self.p2_puzzle_hash,
        })
    }
}
