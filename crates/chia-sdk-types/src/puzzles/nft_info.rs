use chia_protocol::{Bytes32, Coin};
use chia_puzzles::Proof;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NftInfo<M> {
    pub launcher_id: Bytes32,
    pub coin: Coin,
    pub nft_inner_puzzle_hash: Bytes32,
    pub p2_puzzle_hash: Bytes32,
    pub proof: Proof,
    pub metadata: M,
    pub metadata_updater_hash: Bytes32,
    pub current_owner: Option<Bytes32>,
    pub royalty_puzzle_hash: Bytes32,
    pub royalty_percentage: u16,
}

impl<M> NftInfo<M> {
    pub fn with_metadata<N>(self, metadata: N) -> NftInfo<N> {
        NftInfo {
            launcher_id: self.launcher_id,
            coin: self.coin,
            nft_inner_puzzle_hash: self.nft_inner_puzzle_hash,
            p2_puzzle_hash: self.p2_puzzle_hash,
            proof: self.proof,
            metadata,
            metadata_updater_hash: self.metadata_updater_hash,
            current_owner: self.current_owner,
            royalty_puzzle_hash: self.royalty_puzzle_hash,
            royalty_percentage: self.royalty_percentage,
        }
    }
}
