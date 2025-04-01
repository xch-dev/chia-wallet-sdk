use crate::{
    DelegationLayer, DriverError, Layer, NftStateLayer, OracleLayer, SingletonLayer, SpendContext,
};
use chia_protocol::{Bytes, Bytes32};
use chia_puzzle_types::nft::NftStateLayerArgs;
use chia_sdk_types::{
    puzzles::{
        DelegationLayerArgs, WriterLayerArgs, DELEGATION_LAYER_PUZZLE_HASH,
        DL_METADATA_UPDATER_PUZZLE_HASH,
    },
    MerkleTree,
};
use clvm_traits::{ClvmDecoder, ClvmEncoder, FromClvm, FromClvmError, Raw, ToClvm, ToClvmError};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use clvmr::Allocator;
use num_bigint::BigInt;

pub type StandardDataStoreLayers<M = DataStoreMetadata, I = DelegationLayer> =
    SingletonLayer<NftStateLayer<M, I>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[repr(u8)]
#[clvm(atom)]
pub enum HintType {
    // 0 skipped to prevent confusion with () which is also none (end of list)
    AdminPuzzle = 1,
    WriterPuzzle = 2,
    OraclePuzzle = 3,
}

impl HintType {
    pub fn from_value(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::AdminPuzzle),
            2 => Some(Self::WriterPuzzle),
            3 => Some(Self::OraclePuzzle),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub enum DelegatedPuzzle {
    Admin(TreeHash),      // puzzle hash
    Writer(TreeHash),     // inner puzzle hash
    Oracle(Bytes32, u64), // oracle fee puzzle hash, fee amount
}

impl DelegatedPuzzle {
    pub fn from_memos(remaining_memos: &mut Vec<Bytes>) -> Result<Self, DriverError> {
        if remaining_memos.len() < 2 {
            return Err(DriverError::MissingMemo);
        }

        let first_memo = remaining_memos.remove(0);
        if first_memo.len() != 1 {
            return Err(DriverError::InvalidMemo);
        }
        let puzzle_type = HintType::from_value(first_memo[0]);

        // under current specs, first value will always be a puzzle hash
        let puzzle_hash: TreeHash = TreeHash::new(
            remaining_memos
                .remove(0)
                .to_vec()
                .try_into()
                .map_err(|_| DriverError::InvalidMemo)?,
        );

        match puzzle_type {
            Some(HintType::AdminPuzzle) => Ok(DelegatedPuzzle::Admin(puzzle_hash)),
            Some(HintType::WriterPuzzle) => Ok(DelegatedPuzzle::Writer(puzzle_hash)),
            Some(HintType::OraclePuzzle) => {
                if remaining_memos.is_empty() {
                    return Err(DriverError::MissingMemo);
                }

                // puzzle hash bech32m_decode(oracle_address), not puzzle hash of the whole oracle puzze!
                let oracle_fee: u64 = BigInt::from_signed_bytes_be(&remaining_memos.remove(0))
                    .to_u64_digits()
                    .1[0];

                Ok(DelegatedPuzzle::Oracle(puzzle_hash.into(), oracle_fee))
            }
            None => Err(DriverError::MissingMemo),
        }
    }
}

pub trait MetadataWithRootHash {
    fn root_hash(&self) -> Bytes32;
    fn root_hash_only(root_hash: Bytes32) -> Self;
}

impl MetadataWithRootHash for DataStoreMetadata {
    fn root_hash(&self) -> Bytes32 {
        self.root_hash
    }

    fn root_hash_only(root_hash: Bytes32) -> Self {
        Self {
            root_hash,
            label: None,
            description: None,
            bytes: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DataStoreMetadata {
    pub root_hash: Bytes32,
    pub label: Option<String>,
    pub description: Option<String>,
    pub bytes: Option<u64>,
}

impl<N, D: ClvmDecoder<Node = N>> FromClvm<D> for DataStoreMetadata {
    fn from_clvm(decoder: &D, node: N) -> Result<Self, FromClvmError> {
        let (root_hash, items) = <(Bytes32, Vec<(String, Raw<N>)>)>::from_clvm(decoder, node)?;
        let mut metadata = Self::root_hash_only(root_hash);

        for (key, Raw(ptr)) in items {
            match key.as_str() {
                "l" => metadata.label = Some(String::from_clvm(decoder, ptr)?),
                "d" => metadata.description = Some(String::from_clvm(decoder, ptr)?),
                "b" => metadata.bytes = Some(u64::from_clvm(decoder, ptr)?),
                _ => (),
            }
        }

        Ok(metadata)
    }
}

impl<N, E: ClvmEncoder<Node = N>> ToClvm<E> for DataStoreMetadata {
    fn to_clvm(&self, encoder: &mut E) -> Result<N, ToClvmError> {
        let mut items: Vec<(&str, Raw<N>)> = Vec::new();

        if let Some(label) = &self.label {
            items.push(("l", Raw(label.to_clvm(encoder)?)));
        }

        if let Some(description) = &self.description {
            items.push(("d", Raw(description.to_clvm(encoder)?)));
        }

        if let Some(bytes) = self.bytes {
            items.push(("b", Raw(bytes.to_clvm(encoder)?)));
        }

        (self.root_hash, items).to_clvm(encoder)
    }
}

#[must_use]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataStoreInfo<M = DataStoreMetadata> {
    pub launcher_id: Bytes32,
    pub metadata: M,
    pub owner_puzzle_hash: Bytes32,
    pub delegated_puzzles: Vec<DelegatedPuzzle>,
}

impl<M> DataStoreInfo<M> {
    pub fn new(
        launcher_id: Bytes32,
        metadata: M,
        owner_puzzle_hash: Bytes32,
        delegated_puzzles: Vec<DelegatedPuzzle>,
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
            delegated_puzzles,
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
            delegated_puzzles: vec![],
        }
    }

    pub fn into_layers_with_delegation_layer(
        self,
        ctx: &mut SpendContext,
    ) -> Result<StandardDataStoreLayers<M, DelegationLayer>, DriverError> {
        Ok(SingletonLayer::new(
            self.launcher_id,
            NftStateLayer::new(
                self.metadata,
                DL_METADATA_UPDATER_PUZZLE_HASH.into(),
                DelegationLayer::new(
                    self.launcher_id,
                    self.owner_puzzle_hash,
                    get_merkle_tree(ctx, self.delegated_puzzles)?.root(),
                ),
            ),
        ))
    }

    #[must_use]
    pub fn into_layers_without_delegation_layer<I>(
        self,
        innermost_layer: I,
    ) -> StandardDataStoreLayers<M, I> {
        SingletonLayer::new(
            self.launcher_id,
            NftStateLayer::new(
                self.metadata,
                DL_METADATA_UPDATER_PUZZLE_HASH.into(),
                innermost_layer,
            ),
        )
    }

    pub fn inner_puzzle_hash(&self, ctx: &mut SpendContext) -> Result<TreeHash, DriverError>
    where
        M: ToClvm<Allocator>,
    {
        let metadata_ptr = ctx.alloc(&self.metadata)?;

        if !self.delegated_puzzles.is_empty() {
            return Ok(NftStateLayerArgs::curry_tree_hash(
                ctx.tree_hash(metadata_ptr),
                CurriedProgram {
                    program: DELEGATION_LAYER_PUZZLE_HASH,
                    args: DelegationLayerArgs {
                        mod_hash: DELEGATION_LAYER_PUZZLE_HASH.into(),
                        launcher_id: self.launcher_id,
                        owner_puzzle_hash: self.owner_puzzle_hash,
                        merkle_root: get_merkle_tree(ctx, self.delegated_puzzles.clone())?.root(),
                    },
                }
                .tree_hash(),
            ));
        }

        let inner_ph_hash: TreeHash = self.owner_puzzle_hash.into();
        Ok(NftStateLayerArgs::curry_tree_hash(
            ctx.tree_hash(metadata_ptr),
            inner_ph_hash,
        ))
    }
}

pub fn get_merkle_tree(
    ctx: &mut SpendContext,
    delegated_puzzles: Vec<DelegatedPuzzle>,
) -> Result<MerkleTree, DriverError> {
    let mut leaves = Vec::<Bytes32>::with_capacity(delegated_puzzles.len());

    for dp in delegated_puzzles {
        match dp {
            DelegatedPuzzle::Admin(puzzle_hash) => {
                leaves.push(puzzle_hash.into());
            }
            DelegatedPuzzle::Writer(inner_puzzle_hash) => {
                leaves.push(WriterLayerArgs::curry_tree_hash(inner_puzzle_hash).into());
            }
            DelegatedPuzzle::Oracle(oracle_puzzle_hash, oracle_fee) => {
                let oracle_full_puzzle_ptr = OracleLayer::new(oracle_puzzle_hash, oracle_fee)
                    .ok_or(DriverError::OddOracleFee)?
                    .construct_puzzle(ctx)?;

                leaves.push(ctx.tree_hash(oracle_full_puzzle_ptr).into());
            }
        }
    }

    Ok(MerkleTree::new(&leaves))
}
