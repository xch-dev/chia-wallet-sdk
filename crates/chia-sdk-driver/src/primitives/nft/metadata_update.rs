use chia_protocol::Bytes32;
use chia_sdk_types::{
    conditions::{NewMetadataInfo, NewMetadataOutput},
    puzzles::NftMetadataUpdater,
    run_puzzle,
};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::tree_hash;
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Spend, SpendContext};

/// An update that prepends a URI to an NFT's [`NftMetadata`](chia_puzzle_types::nft::NftMetadata)
/// using the default metadata updater puzzle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataUpdate {
    /// Which list of URIs to add to.
    pub kind: UriKind,
    /// The URI to prepend to the list.
    pub uri: String,
}

/// The kind of URI in an NFT's [`NftMetadata`](chia_puzzle_types::nft::NftMetadata).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UriKind {
    /// A URI pointing to the NFT's data, such as an image.
    Data,
    /// A URI pointing to the NFT's off-chain metadata JSON.
    Metadata,
    /// A URI pointing to the NFT's license.
    License,
}

impl MetadataUpdate {
    /// Creates a spend of the default metadata updater puzzle that applies this update.
    ///
    /// The result can be passed to [`Nft::transfer_with_metadata`](crate::Nft::transfer_with_metadata)
    /// or used in an `UPDATE_NFT_METADATA` condition. It only works for NFTs that use the default
    /// metadata updater.
    pub fn spend(&self, ctx: &mut SpendContext) -> Result<Spend, DriverError> {
        let solution = ctx.alloc(&match self.kind {
            UriKind::Data => ("u", &self.uri),
            UriKind::Metadata => ("mu", &self.uri),
            UriKind::License => ("lu", &self.uri),
        })?;
        Ok(Spend::new(ctx.alloc_mod::<NftMetadataUpdater>()?, solution))
    }
}

/// Runs an NFT metadata updater puzzle and returns the new metadata and updater puzzle hash.
///
/// This mirrors what the [`NftStateLayer`](crate::NftStateLayer) does when it sees an
/// `UPDATE_NFT_METADATA` condition. Returns [`DriverError::MetadataUpdaterPuzzleHashMismatch`]
/// if `updater_puzzle_reveal` doesn't hash to `current_metadata_updater_puzzle_hash`.
pub fn run_metadata_updater<M>(
    allocator: &mut Allocator,
    current_metadata: &M,
    current_metadata_updater_puzzle_hash: Bytes32,
    updater_puzzle_reveal: NodePtr,
    updater_solution: NodePtr,
) -> Result<NewMetadataInfo<M>, DriverError>
where
    M: ToClvm<Allocator> + FromClvm<Allocator>,
{
    if tree_hash(allocator, updater_puzzle_reveal) != current_metadata_updater_puzzle_hash.into() {
        return Err(DriverError::MetadataUpdaterPuzzleHashMismatch);
    }

    let metadata_updater_solution: Vec<NodePtr> = vec![
        current_metadata.to_clvm(allocator)?,
        current_metadata_updater_puzzle_hash.to_clvm(allocator)?,
        updater_solution,
    ];
    let metadata_updater_solution = metadata_updater_solution.to_clvm(allocator)?;

    let output = run_puzzle(allocator, updater_puzzle_reveal, metadata_updater_solution)?;
    Ok(NewMetadataOutput::<M, NodePtr>::from_clvm(allocator, output)?.metadata_info)
}
