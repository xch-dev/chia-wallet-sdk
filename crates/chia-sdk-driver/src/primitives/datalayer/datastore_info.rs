use crate::{
    DelegationLayer, DelegationLayerArgs, DriverError, Layer, MerkleTree, NftStateLayer,
    OracleLayer, SingletonLayer, SpendContext, WriterLayerArgs, DELEGATION_LAYER_PUZZLE_HASH,
    DL_METADATA_UPDATER_PUZZLE_HASH,
};
use chia_protocol::{Bytes, Bytes32};
use chia_puzzles::nft::NftStateLayerArgs;
use clvm_traits::{ClvmDecoder, ClvmEncoder, FromClvm, FromClvmError, Raw, ToClvm, ToClvmError};
use clvm_utils::{tree_hash, CurriedProgram, ToTreeHash, TreeHash};
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
    pub fn value(&self) -> u8 {
        *self as u8
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

        let puzzle_type: u8 = u8::try_from(
            BigInt::from_signed_bytes_be(
                &remaining_memos
                    .drain(0..1)
                    .next()
                    .ok_or(DriverError::InvalidMemo)?,
            )
            .to_u32_digits()
            .1[0],
        )
        .map_err(|_| DriverError::InvalidMemo)?;

        // under current specs, first value will always be a puzzle hash
        let puzzle_hash: TreeHash = TreeHash::new(
            remaining_memos
                .drain(0..1)
                .next()
                .ok_or(DriverError::MissingMemo)?
                .to_vec()
                .try_into()
                .map_err(|_| DriverError::InvalidMemo)?,
        );

        match puzzle_type {
            _ if puzzle_type == HintType::AdminPuzzle.value() => {
                Ok(DelegatedPuzzle::Admin(puzzle_hash))
            }
            _ if puzzle_type == HintType::WriterPuzzle.value() => {
                Ok(DelegatedPuzzle::Writer(puzzle_hash))
            }
            _ if puzzle_type == HintType::OraclePuzzle.value() => {
                if remaining_memos.is_empty() {
                    return Err(DriverError::MissingMemo);
                }

                // puzzle hash bech32m_decode(oracle_address), not puzzle hash of the whole oracle puzze!
                let oracle_fee: u64 = BigInt::from_signed_bytes_be(
                    &remaining_memos
                        .drain(0..1)
                        .next()
                        .ok_or(DriverError::MissingMemo)?,
                )
                .to_u64_digits()
                .1[0];

                Ok(DelegatedPuzzle::Oracle(puzzle_hash.into(), oracle_fee))
            }
            _ => Err(DriverError::MissingMemo),
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
                    get_merkle_tree(ctx, self.delegated_puzzles)?.root,
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
        M: ToTreeHash,
    {
        if !self.delegated_puzzles.is_empty() {
            return Ok(NftStateLayerArgs::curry_tree_hash(
                self.metadata.tree_hash(),
                CurriedProgram {
                    program: DELEGATION_LAYER_PUZZLE_HASH,
                    args: DelegationLayerArgs {
                        mod_hash: DELEGATION_LAYER_PUZZLE_HASH.into(),
                        launcher_id: self.launcher_id,
                        owner_puzzle_hash: self.owner_puzzle_hash,
                        merkle_root: get_merkle_tree(ctx, self.delegated_puzzles.clone())?.root,
                    },
                }
                .tree_hash(),
            ));
        }

        let inner_ph_hash: TreeHash = self.owner_puzzle_hash.into();
        Ok(NftStateLayerArgs::curry_tree_hash(
            self.metadata.tree_hash(),
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
                    .ok_or(DriverError::Custom("oracle fee must be even".to_string()))?
                    .construct_puzzle(ctx)?;

                leaves.push(tree_hash(&ctx.allocator, oracle_full_puzzle_ptr).into());
            }
        }
    }

    Ok(MerkleTree::new(&leaves))
}
