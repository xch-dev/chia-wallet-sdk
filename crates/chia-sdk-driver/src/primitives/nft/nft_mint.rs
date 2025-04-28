use chia_protocol::Bytes32;
use chia_puzzles::NFT_METADATA_UPDATER_DEFAULT_HASH;

use super::DidOwner;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NftMint<M> {
    pub metadata: M,
    pub metadata_updater_puzzle_hash: Bytes32,
    pub royalty_puzzle_hash: Bytes32,
    pub royalty_ten_thousandths: u16,
    pub p2_puzzle_hash: Bytes32,
    pub owner: Option<DidOwner>,
}

impl<M> NftMint<M> {
    pub fn new(
        metadata: M,
        p2_puzzle_hash: Bytes32,
        royalty_ten_thousandths: u16,
        owner: Option<DidOwner>,
    ) -> Self {
        Self {
            metadata,
            metadata_updater_puzzle_hash: NFT_METADATA_UPDATER_DEFAULT_HASH.into(),
            royalty_puzzle_hash: p2_puzzle_hash,
            royalty_ten_thousandths,
            p2_puzzle_hash,
            owner,
        }
    }

    #[must_use]
    pub fn with_royalty_puzzle_hash(self, royalty_puzzle_hash: Bytes32) -> Self {
        Self {
            royalty_puzzle_hash,
            ..self
        }
    }

    #[must_use]
    pub fn with_custom_metadata_updater(self, metadata_updater_puzzle_hash: Bytes32) -> Self {
        Self {
            metadata_updater_puzzle_hash,
            ..self
        }
    }
}
