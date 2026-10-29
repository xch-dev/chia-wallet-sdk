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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataUpdate {
    pub kind: UriKind,
    pub uri: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UriKind {
    Data,
    Metadata,
    License,
}

impl MetadataUpdate {
    pub fn spend(&self, ctx: &mut SpendContext) -> Result<Spend, DriverError> {
        let solution = ctx.alloc(&match self.kind {
            UriKind::Data => ("u", &self.uri),
            UriKind::Metadata => ("mu", &self.uri),
            UriKind::License => ("lu", &self.uri),
        })?;
        Ok(Spend::new(ctx.alloc_mod::<NftMetadataUpdater>()?, solution))
    }
}

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
