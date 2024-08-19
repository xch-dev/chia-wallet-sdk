use crate::{
    DelegationLayer, MerkleTree, NftStateLayer, SingletonLayer, WriterLayerArgs,
    DL_METADATA_UPDATER_PUZZLE_HASH,
};
use chia_protocol::{Bytes, Bytes32};
use chia_sdk_types::{CreateCoin, CreatePuzzleAnnouncement};
use clvm_traits::{
    clvm_quote, ClvmDecoder, ClvmEncoder, FromClvm, FromClvmError, Raw, ToClvm, ToClvmError,
};
use clvm_utils::ToTreeHash;
use clvmr::{Allocator, NodePtr};

pub type StandardDataStoreLayers<M = DataStoreMetadata, I = DelegationLayer> =
    SingletonLayer<NftStateLayer<M, I>>;

#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub enum DelegatedPuzzle {
    Admin(Bytes32),       // puzzle hash
    Writer(Bytes32),      // inner puzzle hash
    Oracle(Bytes32, u64), // oracle fee puzzle hash, fee amount
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

#[must_use]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataStoreInfo<M = DataStoreMetadata> {
    pub launcher_id: Bytes32,
    pub metadata: M,
    pub owner_puzzle_hash: Bytes32,
    pub delegated_puzzles: Option<Vec<DelegatedPuzzle>>,
}

impl<M> DataStoreInfo<M> {
    pub fn new(
        launcher_id: Bytes32,
        metadata: M,
        owner_puzzle_hash: Bytes32,
        delegated_puzzles: Option<Vec<DelegatedPuzzle>>,
    ) -> Self {
        Self {
            launcher_id,
            metadata,
            owner_puzzle_hash,
            delegated_puzzles,
        }
    }

    pub fn from_layers_with_delegation_layer(
        layers: StandardDataStoreLayers<M, DelegationLayer>,
        delegated_puzzles: Vec<DelegatedPuzzle>,
    ) -> Self {
        Self {
            launcher_id: layers.launcher_id,
            metadata: layers.inner_puzzle.metadata,
            owner_puzzle_hash: layers.inner_puzzle.inner_puzzle.owner_puzzle_hash,
            delegated_puzzles: Some(delegated_puzzles),
        }
    }

    pub fn from_layers_without_delegation_layer<I>(layers: StandardDataStoreLayers<M, I>) -> Self
    where
        I: ToTreeHash,
    {
        Self {
            launcher_id: layers.launcher_id,
            metadata: layers.inner_puzzle.metadata,
            owner_puzzle_hash: layers.inner_puzzle.inner_puzzle.tree_hash().into(),
            delegated_puzzles: None,
        }
    }

    #[must_use]
    pub fn into_layers_with_delegation_layer(self) -> StandardDataStoreLayers<M, DelegationLayer> {
        SingletonLayer::new(
            self.launcher_id,
            NftStateLayer::new(
                self.metadata,
                DL_METADATA_UPDATER_PUZZLE_HASH.into(),
                DelegationLayer::new(
                    self.launcher_id,
                    self.owner_puzzle_hash,
                    self.delegated_puzzles
                        .map(|dp| get_merkle_tree(dp).root_hash)
                        .unwrap_or(Bytes32::default()),
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
}

pub fn get_merkle_tree(delegated_puzzles: Vec<DelegatedPuzzle>) -> MerkleTree {
    let mut leaves = Vec::<Bytes32>::with_capacity(delegated_puzzles.len());

    for dp in delegated_puzzles {
        match dp {
            DelegatedPuzzle::Admin(puzzle_hash) => {
                leaves.push(puzzle_hash.into());
            }
            DelegatedPuzzle::Writer(inner_puzzle_hash) => {
                leaves.push(WriterLayerArgs::curry_tree_hash(inner_puzzle_hash.into()).into());
            }
            DelegatedPuzzle::Oracle(oracle_fee_puzzle_hash, fee_amount) => {
                tree.push(oracle_fee_puzzle_hash.into());
                tree.push(fee_amount.into());
            }
        }
    }

    MerkleTree::new(&leaves)
}