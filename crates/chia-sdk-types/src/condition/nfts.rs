use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};

/// The new metadata and metadata updater puzzle hash returned by an NFT metadata updater.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct NewMetadataInfo<M> {
    /// The metadata after the update.
    pub new_metadata: M,
    /// The puzzle hash of the metadata updater to use for future updates.
    pub new_updater_puzzle_hash: Bytes32,
}

/// The output of an NFT metadata updater puzzle, which is run by the NFT state layer when it
/// sees an [`UpdateNftMetadata`](super::conditions::UpdateNftMetadata) condition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct NewMetadataOutput<M, C> {
    /// The new metadata and metadata updater puzzle hash.
    pub metadata_info: NewMetadataInfo<M>,
    /// Extra conditions output by the metadata updater, which are added to the spend.
    pub conditions: C,
}

/// A price that an NFT was traded for, used to calculate royalties in a
/// [`TransferNft`](super::conditions::TransferNft) condition.
///
/// The royalty transfer program asserts a puzzle announcement from `puzzle_hash` for a royalty
/// payment of a percentage of `amount`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct TradePrice {
    /// The amount the NFT was traded for, in mojos of the asset.
    pub amount: u64,
    /// The puzzle hash that must announce the royalty payment. This is normally the settlement
    /// payments puzzle for the asset (wrapped in the CAT layer for CATs).
    pub puzzle_hash: Bytes32,
}

impl TradePrice {
    /// Creates a new [`TradePrice`] from its amount and settlement puzzle hash.
    pub fn new(amount: u64, puzzle_hash: Bytes32) -> Self {
        Self {
            amount,
            puzzle_hash,
        }
    }
}
