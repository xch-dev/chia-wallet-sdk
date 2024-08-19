use crate::{DelegationLayer, NftStateLayer, SingletonLayer};
use chia_protocol::Bytes32;
use clvm_traits::{ClvmDecoder, ClvmEncoder, FromClvm, FromClvmError, Raw, ToClvm, ToClvmError};

pub type StandardDataStoreLayers =
    SingletonLayer<NftStateLayer<DataStoreMetadata, DelegationLayer>>;

#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub enum DelegatedPuzzle {
    Admin(Bytes32),  // puzzle hash
    Writer(Bytes32), // inner puzzle hash
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DataStoreMetadata {
    pub root_hash: Bytes32,
    pub label: Option<String>,
    pub description: Option<String>,
    pub bytes: Option<u64>,
}

impl DataStoreMetadata {
    pub fn root_hash_only(root_hash: Bytes32) -> Self {
        Self {
            root_hash,
            label: None,
            description: None,
            bytes: None,
        }
    }
}

impl<N, D: ClvmDecoder<Node = N>> FromClvm<D> for DataStoreMetadata {
    fn from_clvm(decoder: &D, node: N) -> Result<Self, FromClvmError> {
        let items: (Raw<N>, Vec<(String, Raw<N>)>) = FromClvm::from_clvm(decoder, node)?;
        let mut metadata = Self::root_hash_only(FromClvm::from_clvm(decoder, items.0 .0)?);

        for (key, value_ptr) in items.1 {
            match key.as_str() {
                "l" => metadata.label = Some(FromClvm::from_clvm(decoder, value_ptr.0)?),
                "d" => metadata.description = Some(FromClvm::from_clvm(decoder, value_ptr.0)?),
                "b" => metadata.bytes = Some(FromClvm::from_clvm(decoder, value_ptr.0)?),
                _ => (),
            }
        }

        Ok(metadata)
    }
}

impl<N, E: ClvmEncoder<Node = N>> ToClvm<E> for DataStoreMetadata {
    fn to_clvm(&self, encoder: &mut E) -> Result<N, ToClvmError> {
        let mut items: Vec<(&str, Raw<N>)> = Vec::new();

        if self.label.is_some() {
            items.push(("l", Raw(self.label.to_clvm(encoder)?)));
        }

        if self.description.is_some() {
            items.push(("d", Raw(self.description.to_clvm(encoder)?)));
        }

        if let Some(bytes) = self.bytes {
            items.push(("b", Raw(bytes.to_clvm(encoder)?)));
        }

        (Raw(self.root_hash.to_clvm(encoder)?), items).to_clvm(encoder)
    }
}

// #[must_use]
// #[derive(Debug, Clone, Copy, PartialEq, Eq)]
// pub struct NftInfo<M> {
//     pub launcher_id: Bytes32,
//     pub metadata: M,
//     pub metadata_updater_puzzle_hash: Bytes32,
//     pub current_owner: Option<Bytes32>,
//     pub royalty_puzzle_hash: Bytes32,
//     pub royalty_ten_thousandths: u16,
//     pub p2_puzzle_hash: Bytes32,
// }

// impl<M> NftInfo<M> {
//     pub fn new(
//         launcher_id: Bytes32,
//         metadata: M,
//         metadata_updater_puzzle_hash: Bytes32,
//         current_owner: Option<Bytes32>,
//         royalty_puzzle_hash: Bytes32,
//         royalty_ten_thousandths: u16,
//         p2_puzzle_hash: Bytes32,
//     ) -> Self {
//         Self {
//             launcher_id,
//             metadata,
//             metadata_updater_puzzle_hash,
//             current_owner,
//             royalty_puzzle_hash,
//             royalty_ten_thousandths,
//             p2_puzzle_hash,
//         }
//     }

//     pub fn from_layers<I>(layers: StandardNftLayers<M, I>) -> Self
//     where
//         I: ToTreeHash,
//     {
//         Self {
//             launcher_id: layers.launcher_id,
//             metadata: layers.inner_puzzle.metadata,
//             metadata_updater_puzzle_hash: layers.inner_puzzle.metadata_updater_puzzle_hash,
//             current_owner: layers.inner_puzzle.inner_puzzle.current_owner,
//             royalty_puzzle_hash: layers
//                 .inner_puzzle
//                 .inner_puzzle
//                 .transfer_layer
//                 .royalty_puzzle_hash,
//             royalty_ten_thousandths: layers
//                 .inner_puzzle
//                 .inner_puzzle
//                 .transfer_layer
//                 .royalty_ten_thousandths,
//             p2_puzzle_hash: layers
//                 .inner_puzzle
//                 .inner_puzzle
//                 .inner_puzzle
//                 .tree_hash()
//                 .into(),
//         }
//     }

//     #[must_use]
//     pub fn into_layers<I>(self, p2_puzzle: I) -> StandardNftLayers<M, I> {
//         SingletonLayer::new(
//             self.launcher_id,
//             NftStateLayer::new(
//                 self.metadata,
//                 self.metadata_updater_puzzle_hash,
//                 NftOwnershipLayer::new(
//                     self.current_owner,
//                     RoyaltyTransferLayer::new(
//                         self.launcher_id,
//                         self.royalty_puzzle_hash,
//                         self.royalty_ten_thousandths,
//                     ),
//                     p2_puzzle,
//                 ),
//             ),
//         )
//     }

//     pub fn inner_puzzle_hash(&self) -> TreeHash
//     where
//         M: ToTreeHash,
//     {
//         NftStateLayerArgs::curry_tree_hash(
//             self.metadata.tree_hash(),
//             NftOwnershipLayerArgs::curry_tree_hash(
//                 self.current_owner,
//                 CurriedProgram {
//                     program: NFT_ROYALTY_TRANSFER_PUZZLE_HASH,
//                     args: NftRoyaltyTransferPuzzleArgs::new(
//                         self.launcher_id,
//                         self.royalty_puzzle_hash,
//                         self.royalty_ten_thousandths,
//                     ),
//                 }
//                 .tree_hash(),
//                 self.p2_puzzle_hash.into(),
//             ),
//         )
//     }
// }
