use chia_protocol::{Bytes32, Coin};
use chia_puzzles::{
    nft::{NftOwnershipLayerArgs, NftRoyaltyTransferPuzzleArgs, NftStateLayerArgs},
    singleton::SingletonArgs,
    LineageProof, Proof,
};
use clvm_utils::TreeHash;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub struct NftInfo<M> {
    pub launcher_id: Bytes32,
    pub coin: Coin,
    pub inner_puzzle_hash: Bytes32,
    pub p2_puzzle_hash: Bytes32,
    pub proof: Proof,
    pub metadata: M,
    pub metadata_hash: TreeHash,
    pub current_owner: Option<Bytes32>,
    pub royalty_puzzle_hash: Bytes32,
    pub royalty_percentage: u16,
}

impl<M> NftInfo<M> {
    pub fn child(self, p2_puzzle_hash: Bytes32, new_owner: Option<Bytes32>) -> Self {
        let transfer_program = NftRoyaltyTransferPuzzleArgs::curry_tree_hash(
            self.launcher_id,
            self.royalty_puzzle_hash,
            self.royalty_percentage,
        );

        let ownership_layer = NftOwnershipLayerArgs::curry_tree_hash(
            new_owner,
            transfer_program,
            p2_puzzle_hash.into(),
        );

        let state_layer = NftStateLayerArgs::curry_tree_hash(self.metadata_hash, ownership_layer);

        let puzzle_hash = SingletonArgs::curry_tree_hash(self.launcher_id, state_layer);

        Self {
            launcher_id: self.launcher_id,
            coin: Coin::new(self.coin.coin_id(), puzzle_hash.into(), self.coin.amount),
            inner_puzzle_hash: state_layer.into(),
            p2_puzzle_hash,
            proof: Proof::Lineage(LineageProof {
                parent_parent_coin_id: self.coin.parent_coin_info,
                parent_inner_puzzle_hash: self.inner_puzzle_hash,
                parent_amount: self.coin.amount,
            }),
            metadata: self.metadata,
            metadata_hash: self.metadata_hash,
            current_owner: new_owner,
            royalty_puzzle_hash: self.royalty_puzzle_hash,
            royalty_percentage: self.royalty_percentage,
        }
    }

    pub fn with_metadata<N>(self, metadata: N, metadata_hash: TreeHash) -> NftInfo<N> {
        NftInfo {
            launcher_id: self.launcher_id,
            coin: self.coin,
            inner_puzzle_hash: self.inner_puzzle_hash,
            p2_puzzle_hash: self.p2_puzzle_hash,
            proof: self.proof,
            metadata,
            metadata_hash,
            current_owner: self.current_owner,
            royalty_puzzle_hash: self.royalty_puzzle_hash,
            royalty_percentage: self.royalty_percentage,
        }
    }
}
