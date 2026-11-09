use chia_protocol::Bytes32;
use chia_puzzles::NFT_METADATA_UPDATER_DEFAULT_HASH;
use chia_sdk_types::conditions::TransferNft;

use crate::HashedPtr;

/// The parameters for minting an NFT with [`Launcher::mint_nft`](crate::Launcher::mint_nft).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NftMint {
    /// The initial metadata of the NFT, typically an allocated
    /// [`NftMetadata`](chia_puzzle_types::nft::NftMetadata).
    pub metadata: HashedPtr,
    /// The puzzle hash of the metadata updater. See [`NftInfo::metadata_updater_puzzle_hash`](crate::NftInfo::metadata_updater_puzzle_hash).
    pub metadata_updater_puzzle_hash: Bytes32,
    /// The puzzle hash that royalties are paid to in offers involving the NFT.
    pub royalty_puzzle_hash: Bytes32,
    /// The royalty in hundredths of a percent, so 300 is 3%.
    pub royalty_basis_points: u16,
    /// The p2 puzzle hash of the minted NFT. The NFT is hinted with this puzzle hash.
    pub p2_puzzle_hash: Bytes32,
    /// An optional transfer condition to assign an owner (typically a DID) during the mint.
    ///
    /// If this assigns an owner, the owner singleton must output the conditions returned by
    /// [`Launcher::mint_nft`](crate::Launcher::mint_nft).
    pub transfer_condition: Option<TransferNft>,
}

impl NftMint {
    /// Creates a new [`NftMint`] with the default metadata updater, and with royalties
    /// paid to `p2_puzzle_hash`.
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
