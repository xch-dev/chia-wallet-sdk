use chia_protocol::Bytes32;
use chia_puzzles::NFT_METADATA_UPDATER_DEFAULT_HASH;
use chia_sdk_types::conditions::TransferNft;

use crate::HashedPtr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NftMint {
    pub metadata: HashedPtr,
    pub metadata_updater_puzzle_hash: Bytes32,
    pub royalty_puzzle_hash: Bytes32,
    pub royalty_basis_points: u16,
    pub p2_puzzle_hash: Bytes32,
    pub transfer_condition: Option<TransferNft>,
}

impl NftMint {
    pub fn new(
        metadata: HashedPtr,
        p2_puzzle_hash: Bytes32,
        royalty_basis_points: u16,
        transfer_condition: Option<TransferNft>,
    ) -> Self {
        Self {
            metadata,
            metadata_updater_puzzle_hash: NFT_METADATA_UPDATER_DEFAULT_HASH.into(),
            royalty_puzzle_hash: p2_puzzle_hash,
            royalty_basis_points,
            p2_puzzle_hash,
            transfer_condition,
        }
    }
}
