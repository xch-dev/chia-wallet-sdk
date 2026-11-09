use crate::{
    DelegationLayer, DriverError, Layer, NftStateLayer, OracleLayer, SingletonLayer, SpendContext,
};
use chia_protocol::{Bytes, Bytes32};
use chia_puzzle_types::{nft::NftStateLayerArgs, singleton::SingletonArgs};
use chia_puzzles::NFT_STATE_LAYER_HASH;
use chia_sdk_types::{
    MerkleTree, Mod,
    puzzles::{
        DELEGATION_LAYER_PUZZLE_HASH, DL_METADATA_UPDATER_PUZZLE_HASH, DelegationLayerArgs,
        WriterLayerArgs,
    },
};
use clvm_traits::{ClvmDecoder, ClvmEncoder, FromClvm, FromClvmError, Raw, ToClvm, ToClvmError};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use clvmr::Allocator;
use num_bigint::BigInt;

/// The layers of a datastore puzzle: an [`NftStateLayer`] (with metadata `M` and the `DataLayer`
/// metadata updater) wrapped in a [`SingletonLayer`], around the inner layer `I`. The inner layer is
/// either a [`DelegationLayer`] or the owner's puzzle.
pub type StandardDatastoreLayers<M = DatastoreMetadata, I = DelegationLayer> =
    SingletonLayer<NftStateLayer<M, I>>;

/// The type byte that precedes each [`DelegatedPuzzle`] in a datastore's memos.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[repr(u8)]
#[clvm(atom)]
pub enum HintType {
    // 0 skipped to prevent confusion with () which is also none (end of list)
    /// A [`DelegatedPuzzle::Admin`].
    AdminPuzzle = 1,
    /// A [`DelegatedPuzzle::Writer`].
    WriterPuzzle = 2,
    /// A [`DelegatedPuzzle::Oracle`].
    OraclePuzzle = 3,
}

impl HintType {
    /// Converts a type byte into a [`HintType`], or returns [`None`] if it's not recognized.
    pub fn from_value(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::AdminPuzzle),
            2 => Some(Self::WriterPuzzle),
            3 => Some(Self::OraclePuzzle),
            _ => None,
        }
    }
}

/// A puzzle that the owner of a datastore authorizes to spend it, through the [`DelegationLayer`].
#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub enum DelegatedPuzzle {
    /// An admin puzzle with the given puzzle hash, which can update the metadata and the list of
    /// delegated puzzles, but can't change the owner.
    Admin(TreeHash),
    /// A writer, whose inner puzzle hash is wrapped in the [`WriterLayer`](crate::WriterLayer).
    /// It can update the metadata, but can't change the delegated puzzles.
    Writer(TreeHash),
    /// An oracle, which lets anyone spend the datastore by paying the fee (the second value) to
    /// the puzzle hash (the first value). See [`OracleLayer`].
    Oracle(Bytes32, u64),
}

impl DelegatedPuzzle {
    /// Parses a [`DelegatedPuzzle`] from the front of the memos, removing the memos it uses.
    ///
    /// The memos are a [`HintType`] byte, the puzzle hash, and for oracles, the fee.
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
                let fee_memo = remaining_memos.remove(0);
                let oracle_fee: u64 = if fee_memo.is_empty() {
                    0
                } else {
                    BigInt::from_signed_bytes_be(&fee_memo)
                        .try_into()
                        .map_err(|_| DriverError::InvalidMemo)?
                };

                Ok(DelegatedPuzzle::Oracle(puzzle_hash.into(), oracle_fee))
            }
            None => Err(DriverError::MissingMemo),
        }
    }
}

/// Datastore metadata that contains a merkle root hash of the data.
pub trait MetadataWithRootHash {
    /// Returns the root hash of the data.
    fn root_hash(&self) -> Bytes32;
    /// Creates metadata with only a root hash, which is used for datastores launched with the old memo format.
    fn root_hash_only(root_hash: Bytes32) -> Self;
}

impl MetadataWithRootHash for DatastoreMetadata {
    fn root_hash(&self) -> Bytes32 {
        self.root_hash
    }

    fn root_hash_only(root_hash: Bytes32) -> Self {
        Self {
            root_hash,
            label: None,
            description: None,
            bytes: None,
            size_proof: None,
        }
    }
}

/// The standard metadata of a datastore.
///
/// It's encoded as `(root_hash . ((key . value)...))`, with the optional values stored under the
/// keys `"l"`, `"d"`, `"b"`, and `"sp"`. Unknown keys are ignored when parsing.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DatastoreMetadata {
    /// The merkle root hash of the data in the store.
    pub root_hash: Bytes32,
    /// A short label for the store.
    pub label: Option<String>,
    /// A description of the store.
    pub description: Option<String>,
    /// The size of the data, in bytes.
    pub bytes: Option<u64>,
    /// A proof of the size of the data.
    pub size_proof: Option<String>,
}

impl<N, D: ClvmDecoder<Node = N>> FromClvm<D> for DatastoreMetadata {
    fn from_clvm(decoder: &D, node: N) -> Result<Self, FromClvmError> {
        let (root_hash, items) = <(Bytes32, Vec<(String, Raw<N>)>)>::from_clvm(decoder, node)?;
        let mut metadata = Self::root_hash_only(root_hash);

        for (key, Raw(ptr)) in items {
            match key.as_str() {
                "l" => metadata.label = Some(String::from_clvm(decoder, ptr)?),
                "d" => metadata.description = Some(String::from_clvm(decoder, ptr)?),
                "b" => metadata.bytes = Some(u64::from_clvm(decoder, ptr)?),
                "sp" => metadata.size_proof = Some(String::from_clvm(decoder, ptr)?),
                _ => (),
            }
        }

        Ok(metadata)
    }
}

impl<N, E: ClvmEncoder<Node = N>> ToClvm<E> for DatastoreMetadata {
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

        if let Some(size_proof) = &self.size_proof {
            items.push(("sp", Raw(size_proof.to_clvm(encoder)?)));
        }

        (self.root_hash, items).to_clvm(encoder)
    }
}

/// Information needed to construct the outer puzzle of a [`Datastore`](crate::Datastore).
#[must_use]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatastoreInfo<M = DatastoreMetadata> {
    /// The coin id of the launcher coin that created this datastore's singleton.
    pub launcher_id: Bytes32,
    /// The metadata stored in the [`NftStateLayer`].
    pub metadata: M,
    /// The puzzle hash of the owner, who has full control over the datastore.
    pub owner_puzzle_hash: Bytes32,
    /// The puzzles that the owner has delegated access to. If this is empty, there is no
    /// [`DelegationLayer`] and the owner's puzzle is the inner puzzle of the state layer.
    pub delegated_puzzles: Vec<DelegatedPuzzle>,
}

impl<M> DatastoreInfo<M> {
    /// Creates a new [`DatastoreInfo`] from its fields.
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

    /// Creates a [`DatastoreInfo`] from parsed layers that include a [`DelegationLayer`].
    ///
    /// The delegated puzzles aren't in the puzzle (only their merkle root is), so they must be provided.
    pub fn from_layers_with_delegation_layer(
        layers: StandardDatastoreLayers<M, DelegationLayer>,
        delegated_puzzles: Vec<DelegatedPuzzle>,
    ) -> Self {
        Self {
            launcher_id: layers.launcher_id,
            metadata: layers.inner_puzzle.metadata,
            owner_puzzle_hash: layers.inner_puzzle.inner_puzzle.owner_puzzle_hash,
            delegated_puzzles,
        }
    }

    /// Creates a [`DatastoreInfo`] from parsed layers without a [`DelegationLayer`], where the
    /// innermost puzzle is the owner's puzzle.
    pub fn from_layers_without_delegation_layer<I>(layers: StandardDatastoreLayers<M, I>) -> Self
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

    /// Constructs the [`StandardDatastoreLayers`] with a [`DelegationLayer`] for the delegated puzzles.
    pub fn into_layers_with_delegation_layer(
        self,
        ctx: &mut SpendContext,
    ) -> Result<StandardDatastoreLayers<M, DelegationLayer>, DriverError> {
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

    /// Constructs the [`StandardDatastoreLayers`] with the given innermost layer, which is
    /// typically the owner's puzzle.
    #[must_use]
    pub fn into_layers_without_delegation_layer<I>(
        self,
        innermost_layer: I,
    ) -> StandardDatastoreLayers<M, I> {
        SingletonLayer::new(
            self.launcher_id,
            NftStateLayer::new(
                self.metadata,
                DL_METADATA_UPDATER_PUZZLE_HASH.into(),
                innermost_layer,
            ),
        )
    }

    /// Calculates the inner puzzle hash of the singleton, which is the hash of the [`NftStateLayer`].
    pub fn inner_puzzle_hash(&self, ctx: &mut SpendContext) -> Result<TreeHash, DriverError>
    where
        M: ToClvm<Allocator>,
    {
        let metadata_ptr = ctx.alloc(&self.metadata)?;
        let metadata_hash = ctx.tree_hash(metadata_ptr);
        let delegation_layer_puzzle_hash = self.delegation_layer_puzzle_hash(ctx)?;

        Ok(NftStateLayerArgs {
            mod_hash: NFT_STATE_LAYER_HASH.into(),
            metadata: metadata_hash,
            metadata_updater_puzzle_hash: DL_METADATA_UPDATER_PUZZLE_HASH.into(),
            inner_puzzle: delegation_layer_puzzle_hash,
        }
        .curry_tree_hash())
    }

    /// Calculates the full puzzle hash of the singleton.
    pub fn puzzle_hash(&self, ctx: &mut SpendContext) -> Result<TreeHash, DriverError>
    where
        M: ToClvm<Allocator>,
    {
        Ok(SingletonArgs::curry_tree_hash(
            self.launcher_id,
            self.inner_puzzle_hash(ctx)?,
        ))
    }

    /// Checks whether this info produces the given full puzzle hash.
    /// This is used to verify information that is parsed from untrusted hints.
    pub(crate) fn matches_puzzle_hash(&self, puzzle_hash: Bytes32) -> Result<bool, DriverError>
    where
        M: ToClvm<Allocator>,
    {
        let mut ctx = SpendContext::new();
        Ok(self.puzzle_hash(&mut ctx)? == puzzle_hash.into())
    }

    /// Calculates the hash of the inner puzzle of the [`NftStateLayer`].
    ///
    /// This is the hash of the [`DelegationLayer`] if there are delegated puzzles, or the owner
    /// puzzle hash otherwise.
    pub fn delegation_layer_puzzle_hash(
        &self,
        ctx: &mut SpendContext,
    ) -> Result<TreeHash, DriverError>
    where
        M: ToClvm<Allocator>,
    {
        if !self.delegated_puzzles.is_empty() {
            return Ok(CurriedProgram {
                program: DELEGATION_LAYER_PUZZLE_HASH,
                args: DelegationLayerArgs {
                    mod_hash: DELEGATION_LAYER_PUZZLE_HASH.into(),
                    launcher_id: self.launcher_id,
                    owner_puzzle_hash: self.owner_puzzle_hash,
                    merkle_root: get_merkle_tree(ctx, self.delegated_puzzles.clone())?.root(),
                },
            }
            .tree_hash());
        }

        Ok(self.owner_puzzle_hash.into())
    }
}

/// Builds the [`MerkleTree`] of the full puzzle hashes of the delegated puzzles, which is
/// curried into the [`DelegationLayer`].
///
/// Returns [`DriverError::OddOracleFee`] if an oracle has an odd fee.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_oracle_delegated_puzzle_from_memos_zero_fee() -> Result<(), DriverError> {
        let oracle_puzzle_hash = Bytes32::default();
        let mut memos = vec![
            Bytes::new([HintType::OraclePuzzle as u8].into()),
            oracle_puzzle_hash.into(),
            Bytes::new(vec![]),
        ];

        let delegated_puzzle = DelegatedPuzzle::from_memos(&mut memos)?;
        assert_eq!(
            delegated_puzzle,
            DelegatedPuzzle::Oracle(oracle_puzzle_hash, 0)
        );
        assert!(memos.is_empty());

        Ok(())
    }
}
